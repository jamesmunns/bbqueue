#![allow(clippy::result_unit_err)]

use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

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

impl<'a, S: Storage, C: Coord> Producer<'a, S, C> {
    pub fn grant_max_remaining(&self, max: usize) -> Result<GrantW<'a, S, C>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.grant_max_remaining(cap, max)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantW {
            bbq: self.bbq,
            ptr,
            len,
            to_commit: 0,
        })
    }

    pub fn grant_exact(&self, sz: usize) -> Result<GrantW<'a, S, C>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.grant_exact(cap, sz)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantW {
            bbq: self.bbq,
            ptr,
            len,
            to_commit: 0,
        })
    }
}

pub struct Consumer<'a, S, C> {
    bbq: &'a BBQueue<S, C>,
}

impl<'a, S: Storage, C: Coord> Consumer<'a, S, C> {
    pub fn read(&self) -> Result<GrantR<'a, S, C>, ()> {
        let (ptr, _cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.read()?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantR {
            bbq: self.bbq,
            ptr,
            len,
            to_release: 0,
        })
    }
}

pub struct GrantW<'a, S: Storage, C: Coord> {
    bbq: &'a BBQueue<S, C>,
    ptr: NonNull<u8>,
    len: usize,
    to_commit: usize,
}

impl<'a, S: Storage, C: Coord> Deref for GrantW<'a, S, C> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord> DerefMut for GrantW<'a, S, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord> Drop for GrantW<'a, S, C> {
    fn drop(&mut self) {
        let GrantW {
            bbq,
            ptr: _,
            len,
            to_commit,
        } = self;
        let (_, cap) = bbq.sto.ptr_len();
        let len = *len;
        let used = (*to_commit).min(len);
        bbq.cor.commit_inner(cap, len, used);
    }
}

impl<'a, S: Storage, C: Coord> GrantW<'a, S, C> {
    pub fn commit(self, used: usize) {
        let GrantW {
            bbq,
            ptr: _,
            len,
            to_commit: _,
        } = self;
        let (_, cap) = bbq.sto.ptr_len();
        let used = used.min(len);
        bbq.cor.commit_inner(cap, len, used);
    }
}

pub struct GrantR<'a, S: Storage, C: Coord> {
    bbq: &'a BBQueue<S, C>,
    ptr: NonNull<u8>,
    len: usize,
    to_release: usize,
}

impl<'a, S: Storage, C: Coord> Deref for GrantR<'a, S, C> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord> DerefMut for GrantR<'a, S, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord> Drop for GrantR<'a, S, C> {
    fn drop(&mut self) {
        let GrantR {
            bbq,
            ptr: _,
            len,
            to_release,
        } = self;
        let len = *len;
        let used = (*to_release).min(len);
        bbq.cor.release_inner(used);
    }
}

impl<'a, S: Storage, C: Coord> GrantR<'a, S, C> {
    pub fn release(self, used: usize) {
        let GrantR {
            bbq,
            ptr: _,
            len,
            to_release: _,
        } = self;
        let used = used.min(len);
        bbq.cor.release_inner(used);
    }
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

        let buf2: [UnsafeCell<MaybeUninit<u8>>; 64] =
            [const { UnsafeCell::new(MaybeUninit::uninit()) }; 64];
        let bbq2: BBQueue<_, AtomicCoord> = BBQueue::new_with_storage(buf2.as_slice());
        let _ = bbq2.split().unwrap();

        let buf3: Box<[UnsafeCell<MaybeUninit<u8>>]> = {
            let mut v: Vec<UnsafeCell<MaybeUninit<u8>>> = Vec::with_capacity(64);
            unsafe {
                v.set_len(64);
            }
            v.into_boxed_slice()
        };
        let bbq3: BBQueue<_, AtomicCoord> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.split().unwrap();

        assert!(BBQ.split().is_err());
        assert!(bbq2.split().is_err());
        assert!(bbq3.split().is_err());
    }

    #[cfg(feature = "cas-atomics")]
    #[test]
    fn smoke() {
        use std::ops::Deref;

        static BBQ: BBQueue<UnsafeCell<MaybeUninit<[u8; 64]>>, AtomicCoord> = BBQueue::new();
        let (prod, cons) = BBQ.split().unwrap();

        let write_once = &[0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14];
        let mut wgr = prod.grant_exact(8).unwrap();
        wgr.copy_from_slice(write_once);
        wgr.commit(8);

        let rgr = cons.read().unwrap();
        assert_eq!(
            rgr.deref(),
            write_once.as_slice(),
        );
        rgr.release(4);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), &write_once[4..]);
        rgr.release(4);

        assert!(cons.read().is_err());
    }
}
