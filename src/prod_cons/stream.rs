use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    queue::BBQueue,
    traits::{
        bbqhdl::BbqHandle,
        coordination::Coord,
        notifier::{AsyncNotifier, Notifier},
        storage::Storage,
    },
};

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub fn producer(&self) -> Producer<&'_ Self, S, C, N> {
        Producer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn consumer(&self) -> Consumer<&'_ Self, S, C, N> {
        Consumer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    pub fn producer(&self) -> Producer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N> {
        Producer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn consumer(&self) -> Consumer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N> {
        Consumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
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
