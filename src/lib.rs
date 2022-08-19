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

const BLOCK_REF_BLOCK_SIZE: usize = 4096;
const BLOCK_REF_BLOCK_AMOUNT: usize = BLOCK_REF_BLOCK_SIZE / std::mem::size_of::<BlockRef>();

pub struct Awwoc;

unsafe fn allow_block_ref_block() -> Option<NonNull<BlockRef>> {
    let new_ptr = map::map(BLOCK_REF_BLOCK_SIZE)?;

    // we have to allocate some space for the BlockRefs themselves

    let block = new_ptr.cast::<BlockRef>();
    Some(block)
}

unsafe fn allow_inner(layout: std::alloc::Layout) -> Option<NonNull<u8>> {
    // SAFETY: soup

    let mut root = lock(&BLOCK);

    // first, try to find something in the free list
    if let Some(mut free_block) = root.next_free_block {
        let prev_next_ptr = addr_of_mut!(root.next_free_block);
        loop {
            let block_ref_ptr = free_block.as_ptr();
            let block_ref = block_ref_ptr.read();

            if block_ref.size <= layout.size() {
                prev_next_ptr.write(block_ref.next_free_block);
                (*block_ref_ptr).next_free_block = None;
                return NonNull::new(block_ref.start);
            }

            match block_ref.next_free_block {
                Some(block) => free_block = block,
                None => break,
            }
        }
    }

    // nothing free, we have to allocate
    let first_block = match root.first_block {
        Some(block) => block,
        None => {
            let block_ref_block = allow_block_ref_block()?;
            root.first_block = Some(block_ref_block);

            block_ref_block
        }
    };

    let prev_last_block = root.last_block;

    let new_block_ptr = if root.block_count < BLOCK_REF_BLOCK_AMOUNT {
        // just append another block
        let ptr = first_block.as_ptr().add(root.block_count);
        root.last_block = NonNull::new(ptr);
        ptr
    } else {
        let new_block_ref_block = allow_block_ref_block()?;
        let last_ptr = root.last_block?;

        (*last_ptr.as_ptr()).next = Some(new_block_ref_block);

        root.last_block = Some(new_block_ref_block);

        new_block_ref_block.as_ptr()
    };

    let size = layout.size();
    let new_data_ptr = map::map(size)?;

    root.block_count += 1;

    if let Some(prev_last_block) = prev_last_block {
        (*prev_last_block.as_ptr()).next = NonNull::new(new_block_ptr);
    }

    new_block_ptr.write(BlockRef {
        start: new_data_ptr.as_ptr(),
        size,
        next: None,
        next_free_block: None,
    });

    Some(new_data_ptr)
}

unsafe impl GlobalAlloc for Awwoc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        match allow_inner(layout) {
            Some(ptr) => ptr.as_ptr(),
            None => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
        return;

        let mut root = lock(&BLOCK);

        let mut option_block = root.first_block;
        while let Some(block) = option_block {
            let block_ptr = block.as_ptr();

            if (*block_ptr).start == ptr {
                let free = mem::replace(&mut root.next_free_block, Some(block));
                (*block_ptr).next_free_block = free;
                return;
            }

            option_block = (*block_ptr).next;
        }

        let _ = std::io::stderr().write_all("invalid pointer passed to dealloc\n".as_bytes());
        libc::abort();
    }
}

static BLOCK: Mutex<RootNode> = Mutex::new(RootNode {
    first_block: None,
    last_block: None,
    block_count: 0,
    next_free_block: None,
});

struct RootNode {
    first_block: Option<NonNull<BlockRef>>,
    last_block: Option<NonNull<BlockRef>>,
    block_count: usize,
    next_free_block: Option<NonNull<BlockRef>>,
}

unsafe impl Send for RootNode {}
unsafe impl Sync for RootNode {}

#[repr(C)]
struct BlockRef {
    start: *mut u8,
    size: usize,
    next: Option<NonNull<BlockRef>>,
    /// only present on freed blocks
    next_free_block: Option<NonNull<BlockRef>>,
}
