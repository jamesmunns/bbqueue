//! A version of BBQueue built on Cortex-M critical sections.
//! This is useful on thumbv6 targets (Cortex-M0, Cortex-M0+)
//! if your platform does not support atomic compare and swaps.
//!
//! ## Local usage
//!
//! ```rust, no_run
//! use bbqueue::{BBBuffer, consts::*};
//!
//! // Create a buffer with six elements
//! let bb: BBBuffer<U6> = BBBuffer::new();
//! let (mut prod, mut cons) = bb.try_split().unwrap();
//!
//! // Request space for one byte
//! let mut wgr = prod.grant_exact(1).unwrap();
//!
//! // Set the data
//! wgr[0] = 123;
//!
//! assert_eq!(wgr.len(), 1);
//!
//! // Make the data ready for consuming
//! wgr.commit(1);
//!
//! // Read all available bytes
//! let rgr = cons.read().unwrap();
//!
//! assert_eq!(rgr[0], 123);
//!
//! // Release the space for later writes
//! rgr.release(1);
//! ```
//!
//! ## Static usage
//!
//! ```rust, no_run
//! use bbqueue::{BBBuffer, ConstBBBuffer, consts::*};
//!
//! // Create a buffer with six elements
//! static BB: BBBuffer<U6> = BBBuffer( ConstBBBuffer::new() );
//!
//! fn main() {
//!     // Split the bbqueue into producer and consumer halves.
//!     // These halves can be sent to different threads or to
//!     // an interrupt handler for thread safe SPSC usage
//!     let (mut prod, mut cons) = BB.try_split().unwrap();
//!
//!     // Request space for one byte
//!     let mut wgr = prod.grant_exact(1).unwrap();
//!
//!     // Set the data
//!     wgr[0] = 123;
//!
//!     assert_eq!(wgr.len(), 1);
//!
//!     // Make the data ready for consuming
//!     wgr.commit(1);
//!
//!     // Read all available bytes
//!     let rgr = cons.read().unwrap();
//!
//!     assert_eq!(rgr[0], 123);
//!
//!     // Release the space for later writes
//!     rgr.release(1);
//!
//!     // The buffer cannot be split twice
//!     assert!(BB.try_split().is_err());
//! }
//! ```

use crate::{Error, Result};
use core::{
    cell::UnsafeCell,
    cmp::min,
    marker::PhantomData,
    mem::{forget, size_of, transmute, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    slice::from_raw_parts,
    slice::from_raw_parts_mut,
};
use cortex_m::interrupt::free;
pub use generic_array::typenum::consts;
use generic_array::{ArrayLength, GenericArray};

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
pub struct BBBuffer<N: ArrayLength<u8>>(
    // Underlying data storage
    #[doc(hidden)] pub ConstBBBuffer<GenericArray<u8, N>>,
);

// unsafe impl<N> Send for BBBuffer<N: ArrayLength<u8>> {}
unsafe impl<A> Sync for ConstBBBuffer<A> {}

impl<'a, N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// Attempt to split the `BBBuffer` into `Producer` halves to gain access to the
    /// buffer. If buffer has already been split, an error will be returned.
    ///
    /// NOTE: When splitting, the underlying buffer will be explicitly initialized
    /// to zero. This may take a measurable amount of time, depending on the size
    /// of the buffer. This is necessary to prevent undefined behavior.
    ///
    /// NOTE: Takes a critical section while splitting
    ///
    /// ```no_run
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create and split a new buffer
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (prod, cons) = buffer.try_split().unwrap();
    ///
    /// // Not possible to split twice
    /// assert!(buffer.try_split().is_err());
    /// ```
    pub fn try_split(&'a self) -> Result<(Producer<'a, N>, Consumer<'a, N>)> {
        free(|_cs| {
            if self.0.already_split {
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
        })
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
    write: usize,

    /// Where the next byte will be read from
    read: usize,

    /// Used in the inverted case to mark the end of the
    /// readable streak. Otherwise will == unsafe { self.buf.as_mut().len() }.
    /// Writer is responsible for placing this at the correct
    /// place when entering an inverted condition, and Reader
    /// is responsible for moving it back to unsafe { self.buf.as_mut().len() }
    /// when exiting the inverted condition
    last: usize,

    /// Used by the Writer to remember what bytes are currently
    /// allowed to be written to, but are not yet ready to be
    /// read from
    reserve: usize,

    /// Is there an active read grant?
    read_in_progress: bool,

    /// Have we already split?
    already_split: bool,
}

impl<A> ConstBBBuffer<A> {
    /// Create a new constant inner portion of a `BBBuffer`.
    ///
    /// NOTE: This is only necessary to use when creating a `BBBuffer` at static
    /// scope, and is generally never used directly. This process is necessary to
    /// work around current limitations in `const fn`, and will be replaced in
    /// the future.
    ///
    /// ```no_run
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
            write: 0,

            /// Owned by the reader
            read: 0,

            /// Cooperatively owned
            last: size_of::<A>(),

            /// Owned by the Writer, "private"
            reserve: 0,

            /// Owned by the Reader, "private"
            read_in_progress: false,

            already_split: false,
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
    /// NOTE: Takes a critical section while determining the grant.
    /// The critical section is only active for the duration of
    /// this function call.
    ///
    /// ```no_run
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
    /// ```
    pub fn grant_exact(&mut self, sz: usize) -> Result<GrantW<'a, N>> {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            // Writer component. Must never write to `read`,
            // be careful writing to `load`
            let write = inner.write;

            if inner.reserve != write {
                // GRANT IN PROCESS, do not allow further grants
                // until the current one has been completed
                return Err(Error::GrantInProgress);
            }

            let read = inner.read;
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
            inner.reserve = start + sz;

            let c = unsafe { (*inner.buf.get()).as_mut_ptr().cast::<u8>() };
            let d = unsafe { from_raw_parts_mut(c.offset(start as isize), sz) };

            Ok(GrantW {
                buf: d,
                bbq: self.bbq,
            })
        })
    }

    /// Request a writable, contiguous section of memory of up to
    /// `sz` bytes. If a buffer of size `sz` is not available, but
    /// some space (0 < available < sz) is available, then a grant
    /// will be given for the remaining size. If no space is available
    /// for writing, an error will be returned
    ///
    /// NOTE: Takes a critical section while determining the grant.
    /// The critical section is only active for the duration of
    /// this function call.
    ///
    /// ```no_run
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// let (mut prod, cons) = buffer.try_split().unwrap();
    ///
    /// // Successfully obtain and commit a grant of four bytes
    /// let mut grant = prod.grant_max_remaining(4).unwrap();
    /// assert_eq!(grant.buf().len(), 4);
    /// grant.commit(4);
    ///
    /// // Try to obtain a grant of three bytes, get two bytes
    /// let mut grant = prod.grant_max_remaining(3).unwrap();
    /// assert_eq!(grant.buf().len(), 2);
    /// grant.commit(2);
    ///
    /// // Try to obtain a grant of one byte, receive an error
    /// assert!(prod.grant_max_remaining(1).is_err());
    /// ```
    pub fn grant_max_remaining(&mut self, mut sz: usize) -> Result<GrantW<'a, N>> {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            // Writer component. Must never write to `read`,
            // be careful writing to `load`
            let write = inner.write;

            if inner.reserve != write {
                // GRANT IN PROCESS, do not allow further grants
                // until the current one has been completed
                return Err(Error::GrantInProgress);
            }

            let read = inner.read;
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
            inner.reserve = start + sz;

            let c = unsafe { (*inner.buf.get()).as_mut_ptr().cast::<u8>() };
            let d = unsafe { from_raw_parts_mut(c.offset(start as isize), sz) };

            Ok(GrantW {
                buf: d,
                bbq: self.bbq,
            })
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
    /// NOTE: Takes a critical section while determining the read grant.
    /// The critical section is only active for the duration of
    /// this function call.
    ///
    /// ```no_run
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
    /// ```
    pub fn read(&mut self) -> Result<GrantR<'a, N>> {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            if inner.read_in_progress {
                return Err(Error::GrantInProgress);
            }

            let write = inner.write;
            let last = inner.last;
            let mut read = inner.read;

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
                inner.read = 0;
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

            inner.read_in_progress = true;

            let c = unsafe { (*inner.buf.get()).as_ptr().cast::<u8>() };
            let d = unsafe { from_raw_parts(c.offset(read as isize), sz) };

            Ok(GrantR {
                buf: d,
                bbq: self.bbq,
            })
        })
    }
}

// Private impls, used by Queue or Producer/Consumer
impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// Returns the size of the backing storage.
    ///
    /// This is the maximum number of bytes that can be stored in this queue.
    ///
    /// ```no_run
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
    /// assert_eq!(buffer.capacity(), 6);
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
    /// ```no_run
    /// use bbqueue::{BBBuffer, consts::*};
    ///
    /// // Create a new buffer of 6 elements
    /// let buffer: BBBuffer<U6> = BBBuffer::new();
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
#[derive(Debug, PartialEq)]
pub struct GrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    buf: &'a mut [u8],
    bbq: NonNull<BBBuffer<N>>,
}

/// A structure representing a contiguous region of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
///
/// NOTE: If the grant is dropped without explicitly releasing
/// the contents, then no bytes will be released as read.
#[derive(Debug, PartialEq)]
pub struct GrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    buf: &'a [u8],
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
    /// NOTE: Takes a critical section while commiting the grant.
    /// The critical section is only active for the duration of
    /// this function call.
    pub fn commit(mut self, used: usize) {
        self.commit_inner(used);
        forget(self);
    }

    /// Obtain access to the inner buffer for writing
    ///
    /// ```no_run
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
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            // Writer component. Must never write to READ,
            // be careful writing to LAST

            // Saturate the grant commit
            let len = self.buf.len();
            let used = min(len, used);

            let write = inner.write;
            inner.reserve -= len - used;

            let max = N::to_usize();
            let last = inner.last;

            if (inner.reserve < write) && (write != max) {
                // We have already wrapped, but we are skipping some bytes at the end of the ring.
                // Mark `last` where the write pointer used to be to hold the line here
                inner.last = write;
            } else if write > last {
                // We've now passed the last pointer, which was previously the artificial
                // end of the ring. Now that we've passed it, we can "unlock" the section
                // that was previously skipped.
                inner.last = max;
            }
            // else: If write == last, either:
            // * last == max, so no need to write, OR
            // * If we write in the end chunk again, we'll update last to max next time
            // * If we write to the start chunk in a wrap, we'll update last when we
            //     move write backwards

            // Write must be updated AFTER last, otherwise read could think it was
            // time to invert early!
            inner.write = inner.reserve;
        })
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
    /// NOTE: Takes a critical section while releasing the read grant.
    /// The critical section is only active for the duration of
    /// this function call.
    pub fn release(mut self, used: usize) {
        self.release_inner(used);
        forget(self);
    }

    /// Obtain access to the inner buffer for writing
    ///
    /// ```no_run
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
    fn release_inner(&mut self, used: usize) {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            // Saturate the grant release
            let used = min(self.buf.len(), used);

            // This should be fine, purely incrementing
            inner.read += used;

            inner.read_in_progress = false;
        })
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
