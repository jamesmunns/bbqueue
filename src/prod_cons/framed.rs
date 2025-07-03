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

/// # Safety
///
/// Do it right
pub unsafe trait LenHeader: Into<usize> + Copy + Ord {
    type Bytes;
    fn to_le_bytes(&self) -> Self::Bytes;
    fn from_le_bytes(by: Self::Bytes) -> Self;
}

unsafe impl LenHeader for u16 {
    type Bytes = [u8; 2];

    #[inline(always)]
    fn to_le_bytes(&self) -> Self::Bytes {
        u16::to_le_bytes(*self)
    }

    #[inline(always)]
    fn from_le_bytes(by: Self::Bytes) -> Self {
        u16::from_le_bytes(by)
    }
}
unsafe impl LenHeader for usize {
    type Bytes = [u8; core::mem::size_of::<usize>()];

    #[inline(always)]
    fn to_le_bytes(&self) -> Self::Bytes {
        usize::to_le_bytes(*self)
    }

    #[inline(always)]
    fn from_le_bytes(by: Self::Bytes) -> Self {
        usize::from_le_bytes(by)
    }
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub fn framed_producer(&self) -> FramedProducer<&'_ Self> {
        FramedProducer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn framed_consumer(&self) -> FramedConsumer<&'_ Self> {
        FramedConsumer {
            bbq: self.bbq_ref(),
            pd: PhantomData,
        }
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    pub fn framed_producer(&self) -> FramedProducer<std::sync::Arc<BBQueue<S, C, N>>> {
        FramedProducer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn framed_consumer(&self) -> FramedConsumer<std::sync::Arc<BBQueue<S, C, N>>> {
        FramedConsumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }
}

pub struct FramedProducer<Q, H = u16>
where
    Q: BbqHandle,
    H: LenHeader,
{
    bbq: Q::Target,
    pd: PhantomData<H>,
}

impl<Q, H> FramedProducer<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    pub fn grant(&self, sz: H) -> Result<FramedGrantW<Q, H>, ()> {
        let (ptr, cap) = self.bbq.sto.ptr_len();
        let needed = sz.into() + core::mem::size_of::<H>();

        let offset = self.bbq.cor.grant_exact(cap, needed)?;

        let base_ptr = unsafe {
            let p = ptr.as_ptr().byte_add(offset);
            NonNull::new_unchecked(p)
        };
        Ok(FramedGrantW {
            bbq: self.bbq.clone(),
            base_ptr,
            hdr: sz,
        })
    }
}

impl<Q, H> FramedProducer<Q, H>
where
    Q: BbqHandle,
    Q::Notifier: AsyncNotifier,
    H: LenHeader,
{
    pub async fn wait_grant(&self, sz: H) -> FramedGrantW<Q, H> {
        self.bbq.not.wait_for_not_full(|| self.grant(sz).ok()).await
    }
}

pub struct FramedConsumer<Q, H = u16>
where
    Q: BbqHandle,
    H: LenHeader,
{
    bbq: Q::Target,
    pd: PhantomData<H>,
}

impl<Q, H> FramedConsumer<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    pub fn read(&self) -> Result<FramedGrantR<Q, H>, ()> {
        let (ptr, _cap) = self.bbq.sto.ptr_len();
        let (offset, grant_len) = self.bbq.cor.read()?;

        // Calculate the size so we can figure out where the body
        // starts in the grant
        let hdr_sz = const { core::mem::size_of::<H>() };
        if hdr_sz > grant_len {
            // This means that we got a read grant that doesn't even
            // cover the size of a header - this should only be possible
            // if you used a stream producer to create a grant, this is
            // not compatible. We need to release the read grant, and
            // return an error
            self.bbq.cor.release_inner(0);
            return Err(());
        }

        // Ptr is the base of (HDR, Body)
        let ptr = unsafe { ptr.as_ptr().byte_add(offset) };
        // Read the potentially unaligned header
        let hdr: H = unsafe { ptr.cast::<H>().read_unaligned() };
        if (hdr_sz + hdr.into()) > grant_len {
            // Again, the header value + header size are larger than
            // the actual read grant, this means someone is doing
            // something sketch. We need to release the read grant,
            // and return an error
            self.bbq.cor.release_inner(0);
            return Err(());
        }

        // Get the body, which is the base ptr offset by the header size
        let body_ptr = unsafe {
            let p = ptr.byte_add(hdr_sz);
            core::ptr::NonNull::new_unchecked(p)
        };
        Ok(FramedGrantR {
            bbq: self.bbq.clone(),
            body_ptr,
            hdr,
        })
    }
}

impl<Q, H> FramedConsumer<Q, H>
where
    Q: BbqHandle,
    Q::Notifier: AsyncNotifier,
    H: LenHeader,
{
    pub async fn wait_read(&self) -> FramedGrantR<Q, H> {
        self.bbq.not.wait_for_not_empty(|| self.read().ok()).await
    }
}

pub struct FramedGrantW<Q, H = u16>
where
    Q: BbqHandle,
    H: LenHeader,
{
    bbq: Q::Target,
    base_ptr: NonNull<u8>,
    hdr: H,
}

unsafe impl<Q, H> Send for FramedGrantW<Q, H>
where
    Q: BbqHandle,
    Q::Target: Send,
    H: LenHeader + Send
{

}

impl<Q, H> Deref for FramedGrantW<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let len = self.hdr.into();
        let body_ptr = unsafe {
            let hdr_sz = const { core::mem::size_of::<H>() };
            self.base_ptr.as_ptr().byte_add(hdr_sz)
        };
        unsafe { core::slice::from_raw_parts(body_ptr, len) }
    }
}

impl<Q, H> DerefMut for FramedGrantW<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        let len = self.hdr.into();
        let body_ptr = unsafe {
            let hdr_sz = const { core::mem::size_of::<H>() };
            self.base_ptr.as_ptr().byte_add(hdr_sz)
        };
        unsafe { core::slice::from_raw_parts_mut(body_ptr, len) }
    }
}

impl<Q, H> Drop for FramedGrantW<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    fn drop(&mut self) {
        // Default drop performs an "abort"
        let (_ptr, cap) = self.bbq.sto.ptr_len();
        let hdrlen: usize = const { core::mem::size_of::<H>() };
        let grant_len = hdrlen + self.hdr.into();
        self.bbq.cor.commit_inner(cap, grant_len, 0);
    }
}

impl<Q, H> FramedGrantW<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    pub fn commit(self, used: H) {
        let (_ptr, cap) = self.bbq.sto.ptr_len();
        let hdrlen: usize = const { core::mem::size_of::<H>() };
        let grant_len = hdrlen + self.hdr.into();
        let clamp_hdr = self.hdr.min(used);
        let used_len: usize = hdrlen + clamp_hdr.into();

        unsafe {
            self.base_ptr
                .cast::<H>()
                .as_ptr()
                .write_unaligned(clamp_hdr);
        }

        self.bbq.cor.commit_inner(cap, grant_len, used_len);
        self.bbq.not.wake_one_consumer();
        core::mem::forget(self);
    }

    pub fn abort(self) {
        // The default behavior is to abort - do nothing, let the
        // drop impl run
    }
}

pub struct FramedGrantR<Q, H = u16>
where
    Q: BbqHandle,
    H: LenHeader,
{
    bbq: Q::Target,
    body_ptr: NonNull<u8>,
    hdr: H,
}

unsafe impl<Q, H> Send for FramedGrantR<Q, H>
where
    Q: BbqHandle,
    Q::Target: Send,
    H: LenHeader + Send
{

}

impl<Q, H> Deref for FramedGrantR<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let len: usize = self.hdr.into();
        unsafe { core::slice::from_raw_parts(self.body_ptr.as_ptr(), len) }
    }
}

impl<Q, H> DerefMut for FramedGrantR<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        let len: usize = self.hdr.into();
        unsafe { core::slice::from_raw_parts_mut(self.body_ptr.as_ptr(), len) }
    }
}

impl<Q, H> Drop for FramedGrantR<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    fn drop(&mut self) {
        // Default behavior is "keep" - release zero bytes
        self.bbq.cor.release_inner(0);
    }
}

impl<Q, H> FramedGrantR<Q, H>
where
    Q: BbqHandle,
    H: LenHeader,
{
    pub fn release(self) {
        let len: usize = self.hdr.into();
        let hdrlen: usize = const { core::mem::size_of::<H>() };
        let used = len + hdrlen;
        self.bbq.cor.release_inner(used);
        self.bbq.not.wake_one_producer();
        core::mem::forget(self);
    }

    pub fn keep(self) {
        // Default behavior is "keep"
    }
}
