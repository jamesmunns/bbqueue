use core::ptr::NonNull;
use std::{cell::UnsafeCell, mem::MaybeUninit};

pub trait Storage {
    fn ptr_len(&self) -> (NonNull<u8>, usize);
}

pub trait ConstStorage: Storage {
    const INIT: Self;
}

impl<const N: usize> Storage for UnsafeCell<MaybeUninit<[u8; N]>> {
    fn ptr_len(&self) -> (NonNull<u8>, usize) {
        if N == 0 {
            return (NonNull::dangling(), N);
        }
        let ptr: *mut MaybeUninit<[u8; N]> = self.get();
        let ptr: *mut u8 = ptr.cast();
        // SAFETY: UnsafeCell and MaybeUninit are both repr transparent, cast is
        // sound to get to first byte element
        let nn_ptr = unsafe { NonNull::new_unchecked(ptr) };
        (nn_ptr, N)
    }
}

impl<const N: usize> ConstStorage for UnsafeCell<MaybeUninit<[u8; N]>> {
    const INIT: Self = UnsafeCell::new(MaybeUninit::uninit());
}

impl<'a> Storage for &'a [UnsafeCell<MaybeUninit<u8>>] {
    fn ptr_len(&self) -> (NonNull<u8>, usize) {
        let len = self.len();

        let ptr: *const UnsafeCell<MaybeUninit<u8>> = self.as_ptr();
        let ptr: *mut MaybeUninit<u8> = UnsafeCell::raw_get(ptr);
        let ptr: *mut u8 = ptr.cast();
        let nn_ptr = unsafe { NonNull::new_unchecked(ptr) };

        (nn_ptr, len)
    }
}

impl Storage for Box<[UnsafeCell<MaybeUninit<u8>>]> {
    fn ptr_len(&self) -> (NonNull<u8>, usize) {
        let len = self.len();

        let ptr: *const UnsafeCell<MaybeUninit<u8>> = self.as_ptr();
        let ptr: *mut MaybeUninit<u8> = UnsafeCell::raw_get(ptr);
        let ptr: *mut u8 = ptr.cast();
        let nn_ptr = unsafe { NonNull::new_unchecked(ptr) };

        (nn_ptr, len)
    }
}

#[cfg(test)]
mod test {
    use std::{cell::UnsafeCell, mem::MaybeUninit};

    use super::Storage;

    #[test]
    fn provenance_slice() {
        let sli: [UnsafeCell<MaybeUninit<u8>>; 64] =
            [const { UnsafeCell::new(MaybeUninit::uninit()) }; 64];
        let sli = sli.as_slice();
        let (ptr, len) = sli.ptr_len();

        // This test ensures that obtaining the pointer for ptr_len through a single
        // element is sound
        for i in 0..len {
            unsafe {
                ptr.as_ptr().add(i).write(i as u8);
            }
        }
    }
}
