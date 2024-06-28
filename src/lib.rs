#![allow(clippy::result_unit_err)]

use std::{cell::UnsafeCell, mem::MaybeUninit};

use traits::{
    coordination::Coord,
    storage::{ConstStorage, Storage},
};

pub mod traits;

pub struct BBQueue<S, C> {
    sto: S,
    cor: C,
}

unsafe impl<C: Sync, const N: usize> Sync for BBQueue<UnsafeCell<MaybeUninit<[u8; N]>>, C> {}

impl<S: Storage, C: Coord> BBQueue<S, C> {
    pub fn new_with_storage(sto: S) -> Self {
        Self { sto, cor: C::INIT }
    }

    #[allow(clippy::type_complexity)]
    pub fn split(&self) -> Result<(Producer<'_, S, C>, Consumer<'_, S, C>), ()> {
        self.cor.take()?;
        // Taken, we now have exclusive access.
        {
            let (ptr, len) = self.sto.ptr_len();

            // Ensure that all storage bytes have been initialized at least once
            unsafe {
                ptr.as_ptr().write_bytes(0, len);
            }
        }
        // Reset/init the tracking variables
        self.cor.reset();

        Ok((Producer { bbq: self }, Consumer { bbq: self }))
    }
}

#[allow(clippy::new_without_default)]
impl<S: ConstStorage, C: Coord> BBQueue<S, C> {
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
        }
    }
}

pub struct Producer<'a, S, C> {
    bbq: &'a BBQueue<S, C>,
}

pub struct Consumer<'a, S, C> {
    bbq: &'a BBQueue<S, C>,
}

#[cfg(test)]
mod test {
    use std::{cell::UnsafeCell, mem::MaybeUninit};

    use crate::{traits::coordination::cas::AtomicCoord, BBQueue};

    #[cfg(feature = "cas-atomics")]
    #[test]
    fn ux() {
        static BBQ: BBQueue<UnsafeCell<MaybeUninit<[u8; 64]>>, AtomicCoord> = BBQueue::new();
        let _ = BBQ.split().unwrap();

        let buf2: [UnsafeCell<MaybeUninit<u8>>; 64] = [const { UnsafeCell::new(MaybeUninit::uninit())}; 64];
        let bbq2: BBQueue<_, AtomicCoord> = BBQueue::new_with_storage(buf2.as_slice());
        let _ = bbq2.split().unwrap();

        let buf3: Box<[UnsafeCell<MaybeUninit<u8>>]> = {
            let mut v: Vec<UnsafeCell<MaybeUninit<u8>>> = Vec::with_capacity(64);
            unsafe { v.set_len(64); }
            v.into_boxed_slice()
        };
        let bbq3: BBQueue<_, AtomicCoord> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.split().unwrap();

        assert!(BBQ.split().is_err());
        assert!(bbq2.split().is_err());
        assert!(bbq3.split().is_err());
    }
}
