#[cfg(all(unix, not(miri)))]
pub use unix::*;

#[cfg(miri)]
pub use miri::*;

#[cfg(unix)]
mod unix {
    use std::ptr::{self, NonNull};

    pub unsafe fn map(len: usize) -> Option<NonNull<u8>> {
        let prot = libc::PROT_READ | libc::PROT_WRITE;
        let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
        let ptr = libc::mmap(ptr::null_mut(), len, prot, flags, -1, 0).cast();

        if is_invalid(ptr) {
            None
        } else {
            Some(NonNull::new_unchecked(ptr))
        }
    }

    pub unsafe fn unmap(addr: *mut u8, len: usize) {
        libc::munmap(addr.cast(), len);
    }

    pub fn is_invalid(ptr: *mut u8) -> bool {
        ptr.is_null() || ptr.addr() == 0xffffffffffffffff
    }
}

#[cfg(miri)]
mod miri {
    use std::alloc::{GlobalAlloc, System, Layout};
    use std::ptr::NonNull;

    pub unsafe fn map(len: usize) -> Option<NonNull<u8>> {
        NonNull::new(System.alloc_zeroed(Layout::from_size_align(len, 4096).unwrap()))
    }

    pub unsafe fn unmap(addr: *mut u8, len: usize) {
        System.dealloc(addr, std::alloc::Layout::array::<u8>(len).unwrap())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn map_write_unmap() {
        unsafe {
            let ptr = super::map(1000).unwrap();
            let ptr = ptr.as_ptr();

            assert_eq!(ptr.read(), 0);

            ptr.write_volatile(5);
            assert_eq!(ptr.read_volatile(), 5);

            super::unmap(ptr, 1000);
        }
    }
}
