#![allow(clippy::result_unit_err)]
#![cfg_attr(not(any(test, feature = "std")), no_std)]

use core::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use traits::{
    coordination::Coord,
    notifier::{AsyncNotifier, Notifier},
    storage::{ConstStorage, Storage},
};

pub mod traits;

pub struct BBQueue<S, C, N> {
    sto: S,
    cor: C,
    not: N,
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self {
            sto,
            cor: C::INIT,
            not: N::INIT,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn split(&self) -> Result<(Producer<'_, S, C, N>, Consumer<'_, S, C, N>), ()> {
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
impl<S: ConstStorage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

pub struct Producer<'a, S, C, N> {
    bbq: &'a BBQueue<S, C, N>,
}

impl<'a, S: Storage, C: Coord, N: Notifier> Producer<'a, S, C, N> {
    pub fn grant_max_remaining(&self, max: usize) -> Result<GrantW<'a, S, C, N>, ()> {
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

    pub fn grant_exact(&self, sz: usize) -> Result<GrantW<'a, S, C, N>, ()> {
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

pub struct Consumer<'a, S, C, N> {
    bbq: &'a BBQueue<S, C, N>,
}

impl<'a, S: Storage, C: Coord, N: Notifier> Consumer<'a, S, C, N> {
    pub fn read(&self) -> Result<GrantR<'a, S, C, N>, ()> {
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

impl<'a, S: Storage, C: Coord, N: AsyncNotifier> Consumer<'a, S, C, N> {
    pub async fn wait_read(&self) -> GrantR<'a, S, C, N> {
        loop {
            let wait_fut = self.bbq.not.register_wait_not_empty().await;
            if let Ok(g) = self.read() {
                return g;
            }
            wait_fut.await;
        }
    }
}

pub struct GrantW<'a, S: Storage, C: Coord, N: Notifier> {
    bbq: &'a BBQueue<S, C, N>,
    ptr: NonNull<u8>,
    len: usize,
    to_commit: usize,
}

impl<'a, S: Storage, C: Coord, N: Notifier> Deref for GrantW<'a, S, C, N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> DerefMut for GrantW<'a, S, C, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> Drop for GrantW<'a, S, C, N> {
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
        if used != 0 {
            bbq.not.wake_one_consumer();
        }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> GrantW<'a, S, C, N> {
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
        if used != 0 {
            bbq.not.wake_one_consumer();
        }
    }
}

pub struct GrantR<'a, S: Storage, C: Coord, N: Notifier> {
    bbq: &'a BBQueue<S, C, N>,
    ptr: NonNull<u8>,
    len: usize,
    to_release: usize,
}

impl<'a, S: Storage, C: Coord, N: Notifier> Deref for GrantR<'a, S, C, N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> DerefMut for GrantR<'a, S, C, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> Drop for GrantR<'a, S, C, N> {
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
        if used != 0 {
            bbq.not.wake_one_producer();
        }
    }
}

impl<'a, S: Storage, C: Coord, N: Notifier> GrantR<'a, S, C, N> {
    pub fn release(self, used: usize) {
        let GrantR {
            bbq,
            ptr: _,
            len,
            to_release: _,
        } = self;
        let used = used.min(len);
        bbq.cor.release_inner(used);
        if used != 0 {
            bbq.not.wake_one_producer();
        }
    }
}

#[cfg(test)]
mod test {
    use core::{ops::Deref, time::Duration};

    use crate::{
        traits::{coordination::cas::AtomicCoord, notifier::MaiNotSpsc, storage::Inline},
        BBQueue,
    };

    #[cfg(all(feature = "cas-atomics", feature = "std"))]
    #[test]
    fn ux() {
        use crate::traits::{notifier::Blocking, storage::BoxedSlice};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let _ = BBQ.split().unwrap();

        let buf2 = Inline::<64>::new();
        let bbq2: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(&buf2);
        let _ = bbq2.split().unwrap();

        let buf3 = BoxedSlice::new(64);
        let bbq3: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.split().unwrap();

        assert!(BBQ.split().is_err());
        assert!(bbq2.split().is_err());
        assert!(bbq3.split().is_err());
    }

    #[cfg(feature = "cas-atomics")]
    #[test]
    fn smoke() {
        use crate::traits::notifier::Blocking;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let (prod, cons) = BBQ.split().unwrap();

        let write_once = &[0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14];
        let mut wgr = prod.grant_exact(8).unwrap();
        wgr.copy_from_slice(write_once);
        wgr.commit(8);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), write_once.as_slice(),);
        rgr.release(4);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), &write_once[4..]);
        rgr.release(4);

        assert!(cons.read().is_err());
    }

    #[tokio::test]
    async fn asink() {
        static BBQ: BBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> = BBQueue::new();
        let (prod, cons) = BBQ.split().unwrap();

        let rxfut = tokio::task::spawn(async move {
            let rgr = cons.wait_read().await;
            assert_eq!(rgr.deref(), &[1, 2, 3]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant_exact(3).unwrap();
            wgr.copy_from_slice(&[1, 2, 3]);
            wgr.commit(3);
        });

        // todo: timeouts
        rxfut.await.unwrap();
        txfut.await.unwrap();
    }
}
