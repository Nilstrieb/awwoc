#![feature(strict_provenance)]
#![allow(dead_code)]

use std::{
    alloc::GlobalAlloc,
    io::Write,
    mem,
    ptr::{addr_of_mut, null_mut, NonNull},
    sync::{Mutex, MutexGuard},
};

mod map;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock().map_err(|e| e.into_inner()) {
        Ok(t) => t,
        Err(t) => t,
    }
}

fn abort(msg: &str) -> ! {
    let _ = std::io::stderr().write_all(msg.as_bytes());
    unsafe { libc::abort() }
}

const BLOCK_REF_BLOCK_SIZE: usize = 4096;
const BLOCK_REF_BLOCK_AMOUNT: usize = BLOCK_REF_BLOCK_SIZE / std::mem::size_of::<BlockRef>();

/// a terrible allocator that mmaps every single allocation. it's horrible. yeah.
pub struct Awwoc;

static ROOT: Mutex<RootNode> = Mutex::new(RootNode::new());

/// ┌──────────────────────────────────────────────────────────────────────────┐
/// │                     ┌──────────────────────────────────┐                 │
/// │                     │         RootNode                 │                 │
/// │                     │ first_blockref   last_blockref   │                 │
/// │                     └───┬───────────────┬──────────────┘                 │
/// │                         │               │                                │
/// │  ┌──────────────────────┘          ┌────┘                                │
/// │  ▼                                 ▼                                     │
/// │  ┌────────────────────────────┐    ┌─────────────────────────────┐       │
/// │  │BlockRef  BlockRef  BlockRef│    │BlockRef                     │       │
/// │  │sta next  sta next  sta next│    │sta next                     │       │
/// │  │  │   │     │   │     │   │ │    │  │                          │       │
/// │  └──┼───┼───▲─┼───┼───▲─┼───┼─┘    └▲─┼──────────────────────────┘       │
/// │     │   └───┘ │   └───┘ │   └───────┘ │                                  │
/// │     ▼         ▼         ▼             ▼                                  │
/// │ ┌────────┐  ┌───────┐  ┌──────┐       ┌──────────┐                       │
/// │ │ data   │  │ data  │  │ data │       │  data    │                       │
/// │ └────────┘  └───────┘  └──────┘       └──────────┘                       │
/// └──────────────────────────────────────────────────────────────────────────┘
struct RootNode {
    /// A pointer to the first blockref. Must point to a valid block or be None. If last_block is
    /// Some, this must be Some as well.
    first_blockref: Option<NonNull<BlockRef>>,
    /// A pointer to the last blockref. Must point to a valid blockref or be None. If first_block
    /// is Some, this must be Some as well.
    last_blockref: Option<NonNull<BlockRef>>,
    /// The amount of blocks currently stored. If it's bigger than BLOCK_REF_BLOCK_AMOUNT, then
    /// there are multiple blocks of blockrefs around.
    block_count: usize,
    /// The next block in the free list.
    next_free_block: Option<NonNull<BlockRef>>,
}

struct BlockRefBlock {
    start: NonNull<BlockRef>,
    len: usize,
}

impl RootNode {
    const fn new() -> Self {
        Self {
            first_blockref: None,
            last_blockref: None,
            block_count: 0,
            next_free_block: None,
        }
    }

    unsafe fn find_in_free_list(&mut self, size: usize) -> Option<NonNull<u8>> {
        if let Some(mut current_block) = self.next_free_block {
            let mut prev_next_ptr = addr_of_mut!(self.next_free_block);
            loop {
                let block_ref_ptr = current_block.as_ptr();
                let block_ref = block_ref_ptr.read();

                if size <= block_ref.size {
                    // rewire the link to skip the current node
                    prev_next_ptr.write(block_ref.next_free_block);
                    (*block_ref_ptr).next_free_block = None;
                    return NonNull::new(block_ref.start);
                }

                match block_ref.next_free_block {
                    Some(block) => {
                        prev_next_ptr = addr_of_mut!((*block_ref_ptr).next_free_block);
                        current_block = block;
                    }
                    None => break,
                }
            }
        }
        None
    }

    unsafe fn new_blockref(&mut self) -> Option<NonNull<BlockRef>> {
        let last_br_amount = self.block_count % BLOCK_REF_BLOCK_AMOUNT;

        let new_block_ptr = if last_br_amount > 0 {
            // just append another block
            // last_block points the the correct br_block for adding a new br
            // we just need to offset it
            let last_block = self
                .last_blockref
                .unwrap_or_else(|| abort("last_block not found even though count is nonnull\n"));

            let new_br_block = last_block.as_ptr().add(1);

            self.last_blockref = NonNull::new(new_br_block);
            new_br_block
        } else {
            // our current blockref block is full, we need a new one

            let new_block_ref_block = alloc_block_ref_block()?;
            if let Some(last_ptr) = self.last_blockref {
                (*last_ptr.as_ptr()).next = Some(new_block_ref_block);
            }

            self.last_blockref = Some(new_block_ref_block);

            if self.block_count == 0 {
                self.first_blockref = Some(new_block_ref_block);
            }

            self.block_count += 1;

            new_block_ref_block.as_ptr()
        };

        NonNull::new(new_block_ptr)
    }

    unsafe fn alloc_inner(&mut self, layout: std::alloc::Layout) -> Option<NonNull<u8>> {
        // SAFETY: soup

        // first, try to find something in the free list
        if let Some(ptr) = self.find_in_free_list(layout.size()) {
            return Some(ptr);
        }

        // nothing free, we have to allocate

        let prev_last_block = self.last_blockref;

        let new_blockref_ptr = self.new_blockref()?;

        let size = layout.size();
        let new_data_ptr = map::map(size)?;

        self.block_count += 1;

        if let Some(prev_last_block) = prev_last_block {
            (*prev_last_block.as_ptr()).next = Some(new_blockref_ptr);
        }

        new_blockref_ptr.as_ptr().write(BlockRef {
            start: new_data_ptr.as_ptr(),
            size,
            next: None,
            next_free_block: None,
        });

        Some(new_data_ptr)
    }

    fn blockrefs_mut(&mut self) -> impl Iterator<Item = *mut BlockRef> {
        let mut option_block = self.first_blockref;

        std::iter::from_fn(move || {
            if let Some(block) = option_block {
                let block_ptr = block.as_ptr();
                option_block = unsafe { (*block_ptr).next };
                Some(block_ptr)
            } else {
                None
            }
        })
    }

    fn br_blocks(&mut self) -> impl Iterator<Item = *mut BlockRef> {
        let mut index = 0;

        self.blockrefs_mut().filter(move |_| {
            let keep = index % BLOCK_REF_BLOCK_AMOUNT == 0;
            index += 1;
            keep
        })
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        for block_ptr in self.blockrefs_mut() {
            if (*block_ptr).start == ptr {
                let free = mem::replace(&mut self.next_free_block, NonNull::new(block_ptr));
                (*block_ptr).next_free_block = free;
                return;
            }
        }

        abort("invalid pointer passed to dealloc\n");
    }

    unsafe fn cleanup(mut self) {
        for block_ptr in self.blockrefs_mut() {
            map::unmap((*block_ptr).start, BLOCK_REF_BLOCK_SIZE);
        }

        for br_block_ptr in self.br_blocks() {
            map::unmap(br_block_ptr.cast::<u8>(), BLOCK_REF_BLOCK_SIZE);
        }
    }
}

unsafe fn alloc_block_ref_block() -> Option<NonNull<BlockRef>> {
    let new_ptr = map::map(BLOCK_REF_BLOCK_SIZE)?;

    // we have to allocate some space for the BlockRefs themselves

    let block = new_ptr.cast::<BlockRef>();
    Some(block)
}

unsafe impl GlobalAlloc for Awwoc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let mut root = lock(&ROOT);

        match root.alloc_inner(layout) {
            Some(ptr) => ptr.as_ptr(),
            None => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
        let mut root = lock(&ROOT);

        root.dealloc(ptr);
    }
}

// SAFETY: I guess
unsafe impl Send for RootNode {}

#[repr(C)]
struct BlockRef {
    start: *mut u8,
    size: usize,
    next: Option<NonNull<BlockRef>>,
    /// only present on freed blocks
    next_free_block: Option<NonNull<BlockRef>>,
}

#[cfg(test)]
mod tests {
    use std::alloc::Layout;

    use crate::RootNode;

    #[test]
    fn alloc_dealloc() {
        let mut alloc = RootNode::new();
        unsafe {
            let ptr = alloc.alloc_inner(Layout::new::<u64>()).unwrap().as_ptr();

            ptr.write_volatile(6);

            assert_eq!(ptr.read_volatile(), 6);

            alloc.dealloc(ptr);

            alloc.cleanup();
        }
    }

    #[test]
    fn reuse_freed() {
        let mut alloc = RootNode::new();
        unsafe {
            let ptr = alloc.alloc_inner(Layout::new::<u64>()).unwrap().as_ptr();
            let first_addr = ptr.addr();
            alloc.dealloc(ptr);

            let ptr2 = alloc.alloc_inner(Layout::new::<u64>()).unwrap().as_ptr();
            ptr2.write_volatile(10);

            assert_eq!(first_addr, ptr2.addr());

            alloc.cleanup();
        }
    }
}
