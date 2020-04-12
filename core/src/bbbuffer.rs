use crate::{
    framed::{FrameConsumer, FrameProducer},
    Error, Result,
};
use core::{
    cell::UnsafeCell,
    cmp::min,
    marker::PhantomData,
    mem::{forget, transmute, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice::from_raw_parts,
    slice::from_raw_parts_mut,
    sync::atomic::{
        AtomicBool, AtomicUsize,
        Ordering::{AcqRel, Acquire, Release},
    },
};
pub use generic_array::typenum::consts;
use generic_array::{ArrayLength, GenericArray};

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
pub struct BBBuffer<N: ArrayLength<u8>>(
    // Underlying data storage
    #[doc(hidden)] pub ConstBBBuffer<GenericArray<u8, N>>,
);

unsafe impl<A> Sync for ConstBBBuffer<A> {}

impl<'a, N> BBBuffer<N>
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
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create and split a new buffer
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (prod, cons) = buffer.try_split().unwrap();
    ///
    /// // Not possible to split twice
    /// assert!(buffer.try_split().is_err());
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn try_split(&'a self) -> Result<(Producer<'a, N>, Consumer<'a, N>)> {
        if atomic::swap(&self.0.already_split, true, AcqRel) {
            return Err(Error::AlreadySplit);
        } else {
            unsafe {
                // Explicitly zero the data to avoid undefined behavior.
                // This is required, because we hand out references to the buffers,
                // which mean that creating them as references is technically UB for now
                let mu_ptr = self.0.buf.get();
                (*mu_ptr).as_mut_ptr().write_bytes(0u8, 1);

                let nn1 = NonNull::new_unchecked(self as *const _ as *mut _);
                let nn2 = NonNull::new_unchecked(self as *const _ as *mut _);

                Ok((
                    Producer {
                        bbq: nn1,
                        pd: PhantomData,
                    },
                    Consumer {
                        bbq: nn2,
                        pd: PhantomData,
                    },
                ))
            }
        }
    }

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
    pub fn try_split_framed(&'a self) -> Result<(FrameProducer<'a, N>, FrameConsumer<'a, N>)> {
        let (producer, consumer) = self.try_split()?;
        Ok((FrameProducer { producer }, FrameConsumer { consumer }))
    }
}

/// `const-fn` version BBBuffer
///
/// NOTE: This is only necessary to use when creating a `BBBuffer` at static
/// scope, and is generally never used directly. This process is necessary to
/// work around current limitations in `const fn`, and will be replaced in
/// the future.
pub struct ConstBBBuffer<A> {
    buf: UnsafeCell<MaybeUninit<A>>,

    /// Where the next byte will be written
    write: AtomicUsize,

    /// Where the next byte will be read from
    read: AtomicUsize,

    /// Used in the inverted case to mark the end of the
    /// readable streak. Otherwise will == sizeof::<self.buf>().
    /// Writer is responsible for placing this at the correct
    /// place when entering an inverted condition, and Reader
    /// is responsible for moving it back to sizeof::<self.buf>()
    /// when exiting the inverted condition
    last: AtomicUsize,

    /// Used by the Writer to remember what bytes are currently
    /// allowed to be written to, but are not yet ready to be
    /// read from
    reserve: AtomicUsize,

    /// Is there an active read grant?
    read_in_progress: AtomicBool,

    /// Have we already split?
    already_split: AtomicBool,
}

impl<A> ConstBBBuffer<A> {
    /// Create a new constant inner portion of a `BBBuffer`.
    ///
    /// NOTE: This is only necessary to use when creating a `BBBuffer` at static
    /// scope, and is generally never used directly. This process is necessary to
    /// work around current limitations in `const fn`, and will be replaced in
    /// the future.
    ///
    /// ```rust,no_run
    /// use bbqueue::{BBBuffer, ConstBBBuffer, consts::*};
    ///
    /// static BUF: BBBuffer<U6> = BBBuffer( ConstBBBuffer::new() );
    ///
    /// fn main() {
    ///    let (prod, cons) = BUF.try_split().unwrap();
    /// }
    /// ```
    pub const fn new() -> Self {
        Self {
            // This will not be initialized until we split the buffer
            buf: UnsafeCell::new(MaybeUninit::uninit()),

            /// Owned by the writer
            write: AtomicUsize::new(0),

            /// Owned by the reader
            read: AtomicUsize::new(0),

            /// Cooperatively owned
            ///
            /// NOTE: This should generally be initialized as size_of::<self.buf>(), however
            /// this would prevent the structure from being entirely zero-initialized,
            /// and can cause the .data section to be much larger than necessary. By
            /// forcing the `last` pointer to be zero initially, we place the structure
            /// in an "inverted" condition, which will be resolved on the first commited
            /// bytes that are written to the structure.
            ///
            /// When read == last == write, no bytes will be allowed to be read (good), but
            /// write grants can be given out (also good).
            last: AtomicUsize::new(0),

            /// Owned by the Writer, "private"
            reserve: AtomicUsize::new(0),

            /// Owned by the Reader, "private"
            read_in_progress: AtomicBool::new(false),

            already_split: AtomicBool::new(false),
        }
    }
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
pub struct Producer<'a, N>
where
    N: ArrayLength<u8>,
{
    bbq: NonNull<BBBuffer<N>>,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, N> Send for Producer<'a, N> where N: ArrayLength<u8> {}

impl<'a, N> Producer<'a, N>
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
    /// use bbqueue::{BBBuffer, consts::*};
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
    pub fn grant_exact(&mut self, sz: usize) -> Result<GrantW<'a, N>> {
        let inner = unsafe { &self.bbq.as_ref().0 };

        // Writer component. Must never write to `read`,
        // be careful writing to `load`
        let write = inner.write.load(Acquire);

        if inner.reserve.load(Acquire) != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(Error::GrantInProgress);
        }

        let read = inner.read.load(Acquire);
        let max = N::to_usize();
        let already_inverted = write < read;

        let start = if already_inverted {
            if (write + sz) < read {
                // Inverted, room is still available
                write
            } else {
                // Inverted, no room is available
                return Err(Error::InsufficientSize);
            }
        } else {
            if write + sz <= max {
                // Non inverted condition
                write
            } else {
                // Not inverted, but need to go inverted

                // NOTE: We check sz < read, NOT <=, because
                // write must never == read in an inverted condition, since
                // we will then not be able to tell if we are inverted or not
                if sz < read {
                    // Invertible situation
                    0
                } else {
                    // Not invertible, no space
                    return Err(Error::InsufficientSize);
                }
            }
        };

        // Safe write, only viewed by this task
        inner.reserve.store(start + sz, Release);

        // This is sound, as UnsafeCell, MaybeUninit, and GenericArray
        // are all `#[repr(Transparent)]
        let start_of_buf_ptr = inner.buf.get().cast::<u8>();
        let grant_slice =
            unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(start as isize), sz) };

        Ok(GrantW {
            buf: grant_slice,
            bbq: self.bbq,
        })
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
    /// use bbqueue::{BBBuffer, consts::*};
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
    pub fn grant_max_remaining(&mut self, mut sz: usize) -> Result<GrantW<'a, N>> {
        let inner = unsafe { &self.bbq.as_ref().0 };

        // Writer component. Must never write to `read`,
        // be careful writing to `load`
        let write = inner.write.load(Acquire);

        if inner.reserve.load(Acquire) != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(Error::GrantInProgress);
        }

        let read = inner.read.load(Acquire);
        let max = N::to_usize();

        let already_inverted = write < read;

        let start = if already_inverted {
            // In inverted case, read is always > write
            let remain = read - write - 1;

            if remain != 0 {
                sz = min(remain, sz);
                write
            } else {
                // Inverted, no room is available
                return Err(Error::InsufficientSize);
            }
        } else {
            if write != max {
                // Some (or all) room remaining in un-inverted case
                sz = min(max - write, sz);
                write
            } else {
                // Not inverted, but need to go inverted

                // NOTE: We check read > 1, NOT read >= 1, because
                // write must never == read in an inverted condition, since
                // we will then not be able to tell if we are inverted or not
                if read > 1 {
                    sz = min(read - 1, sz);
                    0
                } else {
                    // Not invertible, no space
                    return Err(Error::InsufficientSize);
                }
            }
        };

        // Safe write, only viewed by this task
        inner.reserve.store(start + sz, Release);

        // This is sound, as UnsafeCell, MaybeUninit, and GenericArray
        // are all `#[repr(Transparent)]
        let start_of_buf_ptr = inner.buf.get().cast::<u8>();
        let grant_slice =
            unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(start as isize), sz) };

        Ok(GrantW {
            buf: grant_slice,
            bbq: self.bbq,
        })
    }
}

/// `Consumer` is the primary interface for reading data from a `BBBuffer`.
pub struct Consumer<'a, N>
where
    N: ArrayLength<u8>,
{
    bbq: NonNull<BBBuffer<N>>,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, N> Send for Consumer<'a, N> where N: ArrayLength<u8> {}

impl<'a, N> Consumer<'a, N>
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
    /// use bbqueue::{BBBuffer, consts::*};
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
    /// // Obtain a read grant
    /// let mut grant = cons.read().unwrap();
    /// assert_eq!(grant.buf().len(), 4);
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn read(&mut self) -> Result<GrantR<'a, N>> {
        let inner = unsafe { &self.bbq.as_ref().0 };

        if inner.read_in_progress.load(Acquire) {
            return Err(Error::GrantInProgress);
        }

        let write = inner.write.load(Acquire);
        let last = inner.last.load(Acquire);
        let mut read = inner.read.load(Acquire);

        // Resolve the inverted case or end of read
        if (read == last) && (write < read) {
            read = 0;
            // This has some room for error, the other thread reads this
            // Impact to Grant:
            //   Grant checks if read < write to see if inverted. If not inverted, but
            //     no space left, Grant will initiate an inversion, but will not trigger it
            // Impact to Commit:
            //   Commit does not check read, but if Grant has started an inversion,
            //   grant could move Last to the prior write position
            // MOVING READ BACKWARDS!
            inner.read.store(0, Release);
        }

        let sz = if write < read {
            // Inverted, only believe last
            last
        } else {
            // Not inverted, only believe write
            write
        } - read;

        if sz == 0 {
            return Err(Error::InsufficientSize);
        }

        inner.read_in_progress.store(true, Release);

        // This is sound, as UnsafeCell, MaybeUninit, and GenericArray
        // are all `#[repr(Transparent)]
        let start_of_buf_ptr = inner.buf.get().cast::<u8>();
        let grant_slice = unsafe { from_raw_parts(start_of_buf_ptr.offset(read as isize), sz) };

        Ok(GrantR {
            buf: grant_slice,
            bbq: self.bbq,
        })
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
    /// use bbqueue::{BBBuffer, consts::*};
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
    /// use bbqueue::{BBBuffer, consts::*};
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
        Self(ConstBBBuffer::new())
    }
}

/// A structure representing a contiguous region of memory that
/// may be written to, and potentially "committed" to the queue.
///
/// NOTE: If the grant is dropped without explicitly commiting
/// the contents, then no bytes will be comitted for writing.
/// If the `thumbv6` feature is selected, dropping the grant
/// without committing it takes a short critical section,
#[derive(Debug, PartialEq)]
pub struct GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    pub(crate) buf: &'a mut [u8],
    bbq: NonNull<BBBuffer<N>>,
}

/// A structure representing a contiguous region of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
///
/// NOTE: If the grant is dropped without explicitly releasing
/// the contents, then no bytes will be released as read.
/// If the `thumbv6` feature is selected, dropping the grant
/// without releasing it takes a short critical section,
#[derive(Debug, PartialEq)]
pub struct GrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    pub(crate) buf: &'a [u8],
    bbq: NonNull<BBBuffer<N>>,
}

impl<'a, N> GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Finalizes a writable grant given by `grant()` or `grant_max()`.
    /// This makes the data available to be read via `read()`. This consumes
    /// the grant.
    ///
    /// If `used` is larger than the given grant, the maximum amount will
    /// be commited
    ///
    /// NOTE:  If the `thumbv6` feature is selected, this function takes a short critical
    /// section while committing.
    pub fn commit(mut self, used: usize) {
        self.commit_inner(used);
        forget(self);
    }

    /// Obtain access to the inner buffer for writing
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (mut prod, mut cons) = buffer.try_split().unwrap();
    ///
    /// // Successfully obtain and commit a grant of four bytes
    /// let mut grant = prod.grant_max_remaining(4).unwrap();
    /// grant.buf().copy_from_slice(&[1, 2, 3, 4]);
    /// grant.commit(4);
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn buf(&mut self) -> &mut [u8] {
        self.buf
    }

    /// Sometimes, it's not possible for the lifetimes to check out. For example,
    /// if you need to hand this buffer to a function that expects to receive a
    /// `&'static mut [u8]`, it is not possible for the inner reference to outlive the
    /// grant itself.
    ///
    /// You MUST guarantee that in no cases, the reference that is returned here outlives
    /// the grant itself. Once the grant has been released, referencing the data contained
    /// WILL cause undefined behavior.
    ///
    /// Additionally, you must ensure that a separate reference to this data is not created
    /// to this data, e.g. using `DerefMut` or the `buf()` method of this grant.
    pub unsafe fn as_static_mut_buf(&mut self) -> &'static mut [u8] {
        transmute::<&mut [u8], &'static mut [u8]>(self.buf)
    }

    #[inline(always)]
    fn commit_inner(&mut self, used: usize) {
        let inner = unsafe { &self.bbq.as_ref().0 };

        // Writer component. Must never write to READ,
        // be careful writing to LAST

        // Saturate the grant commit
        let len = self.buf.len();
        let used = min(len, used);

        let write = inner.write.load(Acquire);
        atomic::fetch_sub(&inner.reserve, len - used, AcqRel);

        let max = N::to_usize();
        let last = inner.last.load(Acquire);
        let new_write = inner.reserve.load(Acquire);

        if (new_write < write) && (write != max) {
            // We have already wrapped, but we are skipping some bytes at the end of the ring.
            // Mark `last` where the write pointer used to be to hold the line here
            inner.last.store(write, Release);
        } else if new_write > last {
            // We're about to pass the last pointer, which was previously the artificial
            // end of the ring. Now that we've passed it, we can "unlock" the section
            // that was previously skipped.
            //
            // Since new_write is strictly larger than last, it is safe to move this as
            // the other thread will still be halted by the (about to be updated) write
            // value
            inner.last.store(max, Release);
        }
        // else: If new_write == last, either:
        // * last == max, so no need to write, OR
        // * If we write in the end chunk again, we'll update last to max next time
        // * If we write to the start chunk in a wrap, we'll update last when we
        //     move write backwards

        // Write must be updated AFTER last, otherwise read could think it was
        // time to invert early!
        inner.write.store(new_write, Release);
    }
}

impl<'a, N> GrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Release a sequence of bytes from the buffer, allowing the space
    /// to be used by later writes. This consumes the grant.
    ///
    /// If `used` is larger than the given grant, the full grant will
    /// be released.
    ///
    /// NOTE:  If the `thumbv6` feature is selected, this function takes a short critical
    /// section while releasing.
    pub fn release(mut self, used: usize) {
        // Saturate the grant release
        let used = min(self.buf.len(), used);

        self.release_inner(used);
        forget(self);
    }

    pub(crate) fn shrink(&mut self, len: usize) {
        self.buf = &self.buf[..len];
    }

    /// Obtain access to the inner buffer for reading
    ///
    /// ```
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue::{BBBuffer, consts::*};
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
    pub fn buf(&self) -> &[u8] {
        self.buf
    }

    /// Sometimes, it's not possible for the lifetimes to check out. For example,
    /// if you need to hand this buffer to a function that expects to receive a
    /// `&'static [u8]`, it is not possible for the inner reference to outlive the
    /// grant itself.
    ///
    /// You MUST guarantee that in no cases, the reference that is returned here outlives
    /// the grant itself. Once the grant has been released, referencing the data contained
    /// WILL cause undefined behavior.
    ///
    /// Additionally, you must ensure that a separate reference to this data is not created
    /// to this data, e.g. using `Deref` or the `buf()` method of this grant.
    pub unsafe fn as_static_buf(&self) -> &'static [u8] {
        transmute::<&[u8], &'static [u8]>(self.buf)
    }

    #[inline(always)]
    pub(crate) fn release_inner(&mut self, used: usize) {
        let inner = unsafe { &self.bbq.as_ref().0 };

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.buf.len());

        // This should be fine, purely incrementing
        let _ = atomic::fetch_add(&inner.read, used, Release);

        inner.read_in_progress.store(false, Release);
    }
}

impl<'a, N> Drop for GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    fn drop(&mut self) {
        self.commit_inner(0)
    }
}

impl<'a, N> Drop for GrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    fn drop(&mut self) {
        self.release_inner(0)
    }
}

impl<'a, N> Deref for GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

impl<'a, N> DerefMut for GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    fn deref_mut(&mut self) -> &mut [u8] {
        self.buf
    }
}

impl<'a, N> Deref for GrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

#[cfg(feature = "thumbv6")]
mod atomic {
    use core::sync::atomic::{
        AtomicBool, AtomicUsize,
        Ordering::{self, Acquire, Release},
    };
    use cortex_m::interrupt::free;

    #[inline(always)]
    pub fn fetch_add(atomic: &AtomicUsize, val: usize, _order: Ordering) -> usize {
        free(|_| {
            let prev = atomic.load(Acquire);
            atomic.store(prev.wrapping_add(val), Release);
            prev
        })
    }

    #[inline(always)]
    pub fn fetch_sub(atomic: &AtomicUsize, val: usize, _order: Ordering) -> usize {
        free(|_| {
            let prev = atomic.load(Acquire);
            atomic.store(prev.wrapping_sub(val), Release);
            prev
        })
    }

    #[inline(always)]
    pub fn swap(atomic: &AtomicBool, val: bool, _order: Ordering) -> bool {
        free(|_| {
            let prev = atomic.load(Acquire);
            atomic.store(val, Release);
            prev
        })
    }
}

#[cfg(not(feature = "thumbv6"))]
mod atomic {
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[inline(always)]
    pub fn fetch_add(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
        atomic.fetch_add(val, order)
    }

    #[inline(always)]
    pub fn fetch_sub(atomic: &AtomicUsize, val: usize, order: Ordering) -> usize {
        atomic.fetch_sub(val, order)
    }

    #[inline(always)]
    pub fn swap(atomic: &AtomicBool, val: bool, order: Ordering) -> bool {
        atomic.swap(val, order)
    }
}
