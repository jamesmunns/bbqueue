//! Heap-allocated flavor of BBQueue.

pub use crate::common::{ConstBBBuffer, GrantR, GrantW};
use crate::{
    common::{self, atomic},
    Error, Result,
};
use alloc::{boxed::Box, sync::Arc};
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering::AcqRel},
};
pub use generic_array::typenum::consts;
use generic_array::{ArrayLength, GenericArray};

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
pub struct BBBuffer<N: ArrayLength<u8>>(
    // Underlying data storage
    #[doc(hidden)] pub Box<ConstBBBuffer<GenericArray<u8, N>>>,
);

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// Attempt to split the `BBBuffer` into `Consumer` and `Producer` halves to gain access to the
    /// buffer. If buffer has already been split, an error will be returned.
    ///
    /// NOTE: When splitting, the underlying buffer will be explicitly initialized
    /// to zero. This may take a measurable amount of time, depending on the size
    /// of the buffer. This is necessary to prevent undefined behavior. If the buffer
    /// is placed at `static` scope within the `.bss` region, the explicit initialization
    /// will be elided (as it is already performed as part of memory initialization)
    ///
    /// NOTE:  If the `thumbv6` feature is selected, this function takes a short critical section
    /// while splitting.
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create and split a new buffer
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (prod, cons) = buffer.try_split().unwrap();
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn try_split(self) -> Result<(Producer<N>, Consumer<N>)> {
        if atomic::swap(&self.0.already_split, true, AcqRel) {
            return Err(Error::AlreadySplit);
        }

        unsafe {
            // Explicitly zero the data to avoid undefined behavior.
            // This is required, because we hand out references to the buffers,
            // which mean that creating them as references is technically UB for now
            let mu_ptr = self.0.buf.get();
            (*mu_ptr).as_mut_ptr().write_bytes(0u8, 1);

            let nn: NonNull<_> = Box::leak(self.0).into();
            let dealloc_on_drop = Arc::new(AtomicBool::new(false));

            Ok((
                Producer {
                    inner: common::Producer { bbq: nn },
                    dealloc_on_drop: dealloc_on_drop.clone(),
                },
                Consumer {
                    inner: common::Consumer { bbq: nn },
                    dealloc_on_drop: dealloc_on_drop.clone(),
                },
            ))
        }
    }

    /*
    /// Attempt to split the `BBBuffer` into `FrameConsumer` and `FrameProducer` halves
    /// to gain access to the buffer. If buffer has already been split, an error
    /// will be returned.
    ///
    /// NOTE: When splitting, the underlying buffer will be explicitly initialized
    /// to zero. This may take a measurable amount of time, depending on the size
    /// of the buffer. This is necessary to prevent undefined behavior. If the buffer
    /// is placed at `static` scope within the `.bss` region, the explicit initialization
    /// will be elided (as it is already performed as part of memory initialization)
    ///
    /// NOTE:  If the `thumbv6` feature is selected, this function takes a short critical
    /// section while splitting.
    pub fn try_split_framed(self) -> Result<(FrameProducer<'a, N>, FrameConsumer<'a, N>)> {
        let (producer, consumer) = self.try_split()?;
        Ok((FrameProducer { producer }, FrameConsumer { consumer }))
    }*/
}

/// `Producer` is the primary interface for pushing data into a `BBBuffer`.
/// There are various methods for obtaining a grant to write to the buffer, with
/// different potential tradeoffs. As all grants are required to be a contiguous
/// range of data, different strategies are sometimes useful when making the decision
/// between maximizing usage of the buffer, and ensuring a given grant is successful.
///
/// As a short summary of currently possible grants:
///
/// * `grant_exact(N)`
///   * User will receive a grant `sz == N` (or receive an error)
///   * This may cause a wraparound if a grant of size N is not available
///       at the end of the ring.
///   * If this grant caused a wraparound, the bytes that were "skipped" at the
///       end of the ring will not be available until the reader reaches them,
///       regardless of whether the grant commited any data or not.
///   * Maximum possible waste due to skipping: `N - 1` bytes
/// * `grant_max_remaining(N)`
///   * User will receive a grant `0 < sz <= N` (or receive an error)
///   * This will only cause a wrap to the beginning of the ring if exactly
///       zero bytes are available at the end of the ring.
///   * Maximum possible waste due to skipping: 0 bytes
///
/// See [this github issue](https://github.com/jamesmunns/bbqueue/issues/38) for a
/// discussion of grant methods that could be added in the future.
pub struct Producer<N>
where
    N: ArrayLength<u8>,
{
    inner: common::Producer<N>,
    dealloc_on_drop: Arc<AtomicBool>,
}

unsafe impl<N> Send for Producer<N> where N: ArrayLength<u8> {}

impl<N> Producer<N>
where
    N: ArrayLength<u8>,
{
    /// Request a writable, contiguous section of memory of exactly
    /// `sz` bytes. If the buffer size requested is not available,
    /// an error will be returned.
    ///
    /// This method may cause the buffer to wrap around early if the
    /// requested space is not available at the end of the buffer, but
    /// is available at the beginning
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (mut prod, cons) = buffer.try_split().unwrap();
    ///
    /// // Successfully obtain and commit a grant of four bytes
    /// let mut grant = prod.grant_exact(4).unwrap();
    /// assert_eq!(grant.buf().len(), 4);
    /// grant.commit(4);
    ///
    /// // Try to obtain a grant of three bytes
    /// assert!(prod.grant_exact(3).is_err());
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn grant_exact<'a>(&'a mut self, sz: usize) -> Result<GrantW<'a, N>> {
        self.inner.grant_exact(sz)
    }

    /// Request a writable, contiguous section of memory of up to
    /// `sz` bytes. If a buffer of size `sz` is not available without
    /// wrapping, but some space (0 < available < sz) is available without
    /// wrapping, then a grant will be given for the remaining size at the
    /// end of the buffer. If no space is available for writing, an error
    /// will be returned.
    ///
    /// ```
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (mut prod, mut cons) = buffer.try_split().unwrap();
    ///
    /// // Successfully obtain and commit a grant of four bytes
    /// let mut grant = prod.grant_max_remaining(4).unwrap();
    /// assert_eq!(grant.buf().len(), 4);
    /// grant.commit(4);
    ///
    /// // Release the four initial commited bytes
    /// let mut grant = cons.read().unwrap();
    /// assert_eq!(grant.buf().len(), 4);
    /// grant.release(4);
    ///
    /// // Try to obtain a grant of three bytes, get two bytes
    /// let mut grant = prod.grant_max_remaining(3).unwrap();
    /// assert_eq!(grant.buf().len(), 2);
    /// grant.commit(2);
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn grant_max_remaining<'a>(&'a mut self, sz: usize) -> Result<GrantW<'a, N>> {
        self.inner.grant_max_remaining(sz)
    }
}

/// `Consumer` is the primary interface for reading data from a `BBBuffer`.
pub struct Consumer<N>
where
    N: ArrayLength<u8>,
{
    inner: common::Consumer<N>,
    dealloc_on_drop: Arc<AtomicBool>,
}

unsafe impl<N> Send for Consumer<N> where N: ArrayLength<u8> {}

impl<N> Consumer<N>
where
    N: ArrayLength<u8>,
{
    /// Obtains a contiguous slice of committed bytes. This slice may not
    /// contain ALL available bytes, if the writer has wrapped around. The
    /// remaining bytes will be available after all readable bytes are
    /// released
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (mut prod, mut cons) = buffer.try_split().unwrap();
    ///
    /// // Successfully obtain and commit a grant of four bytes
    /// let mut grant = prod.grant_max_remaining(4).unwrap();
    /// grant.buf().copy_from_slice(&[1, 2, 3, 4]);
    /// grant.commit(4);
    ///
    /// // Obtain a read grant, and copy to a buffer
    /// let mut grant = cons.read().unwrap();
    /// let mut buf = [0u8; 4];
    /// buf.copy_from_slice(grant.buf());
    /// assert_eq!(&buf, &[1, 2, 3, 4]);
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn read<'a>(&'a mut self) -> Result<GrantR<'a, N>> {
        self.inner.read()
    }
}

impl<N> Drop for Producer<N>
where
    N: ArrayLength<u8>,
{
    fn drop(&mut self) {
        if atomic::swap(&self.dealloc_on_drop, true, AcqRel) {
            unsafe {
                Box::from_raw(self.inner.bbq.as_ptr());
            }
        }
    }
}

impl<N> Drop for Consumer<N>
where
    N: ArrayLength<u8>,
{
    fn drop(&mut self) {
        if atomic::swap(&self.dealloc_on_drop, true, AcqRel) {
            unsafe {
                Box::from_raw(self.inner.bbq.as_ptr());
            }
        }
    }
}

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// Returns the size of the backing storage.
    ///
    /// This is the maximum number of bytes that can be stored in this queue.
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// assert_eq!(buffer.capacity(), 6);
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn capacity(&self) -> usize {
        N::to_usize()
    }
}

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// Create a new bbqueue
    ///
    /// NOTE: For creating a bbqueue in static context, see `ConstBBBuffer::new()`.
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::consts::*;
    /// use bbqueue::heap::BBBuffer;
    ///
    /// // Create a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn new() -> Self {
        Self(Box::new(ConstBBBuffer::new()))
    }
}
