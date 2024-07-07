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
    pub fn stream_producer(&self) -> StreamProducer<&'_ Self, S, C, N> {
        StreamProducer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn stream_consumer(&self) -> StreamConsumer<&'_ Self, S, C, N> {
        StreamConsumer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    pub fn stream_producer(&self) -> StreamProducer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N> {
        StreamProducer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn stream_consumer(&self) -> StreamConsumer<std::sync::Arc<BBQueue<S, C, N>>, S, C, N> {
        StreamConsumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }
}

pub struct StreamProducer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    pd: PhantomData<(S, C, N)>,
}

impl<Q, S, C, N> StreamProducer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn grant_max_remaining(&self, max: usize) -> Result<StreamGrantW<Q, S, C, N>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.grant_max_remaining(cap, max)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(StreamGrantW {
            bbq: self.bbq.clone(),
            ptr,
            len,
            to_commit: 0,
        })
    }

    pub fn grant_exact(&self, sz: usize) -> Result<StreamGrantW<Q, S, C, N>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let offset = self.bbq.cor.grant_exact(cap, sz)?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(StreamGrantW {
            bbq: self.bbq.clone(),
            ptr,
            len: sz,
            to_commit: 0,
        })
    }
}

impl<Q, S, C, N> StreamProducer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: AsyncNotifier,
    Q: BbqHandle<S, C, N>,
{
    pub async fn wait_grant_max_remaining(&self, max: usize) -> StreamGrantW<Q, S, C, N> {
        self.bbq
            .not
            .wait_for_not_full(|| self.grant_max_remaining(max).ok())
            .await
    }

    pub async fn wait_grant_exact(&self, sz: usize) -> StreamGrantW<Q, S, C, N> {
        self.bbq
            .not
            .wait_for_not_full(|| self.grant_exact(sz).ok())
            .await
    }
}

pub struct StreamConsumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    bbq: Q::Target,
    pd: PhantomData<(S, C, N)>,
}

impl<Q, S, C, N> StreamConsumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    pub fn read(&self) -> Result<StreamGrantR<Q, S, C, N>, ()> {
        let (ptr, _cap) = self.bbq.sto.ptr_len();
        let (offset, len) = self.bbq.cor.read()?;
        let ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(StreamGrantR {
            bbq: self.bbq.clone(),
            ptr,
            len,
            to_release: 0,
        })
    }
}

impl<Q, S, C, N> StreamConsumer<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: AsyncNotifier,
    Q: BbqHandle<S, C, N>,
{
    pub async fn wait_read(&self) -> StreamGrantR<Q, S, C, N> {
        self.bbq.not.wait_for_not_empty(|| self.read().ok()).await
    }
}

pub struct StreamGrantW<Q, S, C, N>
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

impl<Q, S, C, N> Deref for StreamGrantW<Q, S, C, N>
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

impl<Q, S, C, N> DerefMut for StreamGrantW<Q, S, C, N>
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

impl<Q, S, C, N> Drop for StreamGrantW<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    fn drop(&mut self) {
        let StreamGrantW {
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

impl<Q, S, C, N> StreamGrantW<Q, S, C, N>
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

pub struct StreamGrantR<Q, S, C, N>
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

impl<Q, S, C, N> Deref for StreamGrantR<Q, S, C, N>
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

impl<Q, S, C, N> DerefMut for StreamGrantR<Q, S, C, N>
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

impl<Q, S, C, N> Drop for StreamGrantR<Q, S, C, N>
where
    S: Storage,
    C: Coord,
    N: Notifier,
    Q: BbqHandle<S, C, N>,
{
    fn drop(&mut self) {
        let StreamGrantR {
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

impl<Q, S, C, N> StreamGrantR<Q, S, C, N>
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
