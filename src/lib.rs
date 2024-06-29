#![allow(clippy::result_unit_err)]
#![cfg_attr(not(any(test, feature = "std")), no_std)]

use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use traits::{
    bbqhdl::BbqHandle,
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
    pub fn split_borrowed(
        &self,
    ) -> Result<(Producer<&'_ Self, S, C, N>, Consumer<&'_ Self, S, C, N>), ()> {
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

        Ok((
            Producer {
                bbq: self.bbq_ref(),
                pd: PhantomData,
            },
            Consumer {
                bbq: self.bbq_ref(),
                pd: PhantomData,
            },
        ))
    }
}

#[cfg(feature = "std")]
pub struct ArcBBQueue<S, C, N>(std::sync::Arc<BBQueue<S, C, N>>);

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> ArcBBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self(std::sync::Arc::new(BBQueue::new_with_storage(sto)))
    }

    #[allow(clippy::type_complexity)]
    pub fn split_arc(
        self,
    ) -> Result<
        (
            Producer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N>,
            Consumer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N>,
        ),
        (),
    > {
        self.0.cor.take()?;
        // Taken, we now have exclusive access.
        {
            let (ptr, len) = self.0.sto.ptr_len();

            // Ensure that all storage bytes have been initialized at least once
            unsafe {
                ptr.as_ptr().write_bytes(0, len);
            }
        }
        // Reset/init the tracking variables
        self.0.cor.reset();

        Ok((
            Producer {
                bbq: self.0.bbq_ref(),
                pd: PhantomData,
            },
            Consumer {
                bbq: self.0.bbq_ref(),
                pd: PhantomData,
            },
        ))
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

pub struct Producer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    pd: PhantomData<(S, C, N)>,
}

impl<Q, S, C, N> Producer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn grant_max_remaining(&self, max: usize) -> Result<GrantW<Q, S, C, N>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.grant_max_remaining(cap, max)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantW {
            bbq: self.bbq.clone(),
            ptr,
            len,
            to_commit: 0,
        })
    }

    pub fn grant_exact(&self, sz: usize) -> Result<GrantW<Q, S, C, N>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.grant_exact(cap, sz)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantW {
            bbq: self.bbq.clone(),
            ptr,
            len,
            to_commit: 0,
        })
    }
}

pub struct Consumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    pd: PhantomData<(S, C, N)>,
}

impl<Q, S, C, N> Consumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn read(&self) -> Result<GrantR<Q, S, C, N>, ()> {
        let (ptr, _cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.read()?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(GrantR {
            bbq: self.bbq.clone(),
            ptr,
            len,
            to_release: 0,
        })
    }
}

impl<Q, S, C, N> Consumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: AsyncNotifier,
    Q: BbqHandle<S, C, N>,
{
    pub async fn wait_read(&self) -> GrantR<Q, S, C, N> {
        loop {
            let wait_fut = self.bbq.not.register_wait_not_empty().await;
            if let Ok(g) = self.read() {
                return g;
            }
            wait_fut.await;
        }
    }
}

pub struct GrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    ptr: NonNull<u8>,
    len: usize,
    to_commit: usize,
}

impl<Q, S, C, N> Deref for GrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q, S, C, N> DerefMut for GrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q, S, C, N> Drop for GrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
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

impl<Q, S, C, N> GrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn commit(self, used: usize) {
        let (_, cap) = self.bbq.sto.ptr_len();
        let used = used.min(self.len);
        self.bbq.cor.commit_inner(cap, self.len, used);
        if used != 0 {
            self.bbq.not.wake_one_consumer();
        }
        core::mem::forget(self);
    }
}

pub struct GrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    ptr: NonNull<u8>,
    len: usize,
    to_release: usize,
}

impl<Q, S, C, N> Deref for GrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q, S, C, N> DerefMut for GrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q, S, C, N> Drop for GrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
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

impl<Q, S, C, N> GrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn release(self, used: usize) {
        let used = used.min(self.len);
        self.bbq.cor.release_inner(used);
        if used != 0 {
            self.bbq.not.wake_one_producer();
        }
        core::mem::forget(self);
    }
}

#[cfg(test)]
mod test {
    use core::{ops::Deref, time::Duration};

    use crate::{
        traits::{
            coordination::cas::AtomicCoord,
            notifier::MaiNotSpsc,
            storage::{BoxedSlice, Inline},
        },
        ArcBBQueue, BBQueue,
    };

    #[cfg(all(feature = "cas-atomics", feature = "std"))]
    #[test]
    fn ux() {
        use crate::traits::{notifier::Blocking, storage::BoxedSlice};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let _ = BBQ.split_borrowed().unwrap();

        let buf2 = Inline::<64>::new();
        let bbq2: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(&buf2);
        let _ = bbq2.split_borrowed().unwrap();

        let buf3 = BoxedSlice::new(64);
        let bbq3: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.split_borrowed().unwrap();

        assert!(BBQ.split_borrowed().is_err());
        assert!(bbq2.split_borrowed().is_err());
        assert!(bbq3.split_borrowed().is_err());
    }

    #[cfg(feature = "cas-atomics")]
    #[test]
    fn smoke() {
        use crate::traits::notifier::Blocking;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let (prod, cons) = BBQ.split_borrowed().unwrap();

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
        let (prod, cons) = BBQ.split_borrowed().unwrap();

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

    #[tokio::test]
    async fn arc1() {
        let bbq: ArcBBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> =
            ArcBBQueue::new_with_storage(Inline::new());
        let (prod, cons) = bbq.split_arc().unwrap();

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

    #[tokio::test]
    async fn arc2() {
        let bbq: ArcBBQueue<BoxedSlice, AtomicCoord, MaiNotSpsc> =
            ArcBBQueue::new_with_storage(BoxedSlice::new(64));
        let (prod, cons) = bbq.split_arc().unwrap();

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
