use core::{
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
    pub fn stream_producer(&self) -> StreamProducer<&'_ Self> {
        StreamProducer {
            bbq: self.bbq_ref(),
        }
    }

    pub fn stream_consumer(&self) -> StreamConsumer<&'_ Self> {
        StreamConsumer {
            bbq: self.bbq_ref(),
        }
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    pub fn stream_producer(&self) -> StreamProducer<std::sync::Arc<BBQueue<S, C, N>>> {
        StreamProducer {
            bbq: self.0.bbq_ref(),
        }
    }

    pub fn stream_consumer(&self) -> StreamConsumer<std::sync::Arc<BBQueue<S, C, N>>> {
        StreamConsumer {
            bbq: self.0.bbq_ref(),
        }
    }
}

pub struct StreamProducer<Q>
where
    Q: BbqHandle,
{
    bbq: Q::Target,
}

impl<Q> StreamProducer<Q>
where
    Q: BbqHandle,
{
    pub fn grant_max_remaining(&self, max: usize) -> Result<StreamGrantW<Q>, ()> {
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

    pub fn grant_exact(&self, sz: usize) -> Result<StreamGrantW<Q>, ()> {
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

impl<Q> StreamProducer<Q>
where
    Q: BbqHandle,
    Q::Notifier: AsyncNotifier,
{
    pub async fn wait_grant_max_remaining(&self, max: usize) -> StreamGrantW<Q> {
        self.bbq
            .not
            .wait_for_not_full(|| self.grant_max_remaining(max).ok())
            .await
    }

    pub async fn wait_grant_exact(&self, sz: usize) -> StreamGrantW<Q> {
        self.bbq
            .not
            .wait_for_not_full(|| self.grant_exact(sz).ok())
            .await
    }
}

pub struct StreamConsumer<Q>
where
    Q: BbqHandle,
{
    bbq: Q::Target,
}

impl<Q> StreamConsumer<Q>
where
    Q: BbqHandle,
{
    pub fn read(&self) -> Result<StreamGrantR<Q>, ()> {
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

impl<Q> StreamConsumer<Q>
where
    Q: BbqHandle,
    Q::Notifier: AsyncNotifier,
{
    pub async fn wait_read(&self) -> StreamGrantR<Q> {
        self.bbq.not.wait_for_not_empty(|| self.read().ok()).await
    }
}

pub struct StreamGrantW<Q>
where
    Q: BbqHandle,
{
    bbq: Q::Target,
    ptr: NonNull<u8>,
    len: usize,
    to_commit: usize,
}

impl<Q> Deref for StreamGrantW<Q>
where
    Q: BbqHandle,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q> DerefMut for StreamGrantW<Q>
where
    Q: BbqHandle,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q> Drop for StreamGrantW<Q>
where
    Q: BbqHandle,
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

impl<Q> StreamGrantW<Q>
where
    Q: BbqHandle,
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

pub struct StreamGrantR<Q>
where
    Q: BbqHandle,
{
    bbq: Q::Target,
    ptr: NonNull<u8>,
    len: usize,
    to_release: usize,
}

impl<Q> Deref for StreamGrantR<Q>
where
    Q: BbqHandle,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q> DerefMut for StreamGrantR<Q>
where
    Q: BbqHandle,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<Q> Drop for StreamGrantR<Q>
where
    Q: BbqHandle,
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

impl<Q> StreamGrantR<Q>
where
    Q: BbqHandle,
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
