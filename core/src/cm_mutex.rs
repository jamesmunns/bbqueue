//! A version of BBQueue built on Cortex-M critical sections.
//! This is useful on thumbv6 targets (Cortex-M0, Cortex-M0+)
//! if your platform does not support atomic compare and swaps.

pub use generic_array::typenum::consts;
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::{size_of, MaybeUninit, forget},
    ops::{Deref, DerefMut},
    ptr::NonNull,
    result::Result as CoreResult,
    slice::from_raw_parts,
    slice::from_raw_parts_mut,
};
use generic_array::{ArrayLength, GenericArray};
use cortex_m::interrupt::free;

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
pub struct BBBuffer<N: ArrayLength<u8>> (
    // Underlying data storage
    #[doc(hidden)] pub ConstBBBuffer<GenericArray<u8, N>>,
);

// unsafe impl<N> Send for BBBuffer<N: ArrayLength<u8>> {}
unsafe impl<A> Sync for ConstBBBuffer<A> {}

impl<'a, N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    /// NOTE: Takes a critical section while splitting
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

pub struct Producer<'a, N>
where
    N: ArrayLength<u8>,
{
    bbq: NonNull<BBBuffer<N>>,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, N> Send for Producer<'a, N>
where
    N: ArrayLength<u8>
{ }

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
    pub fn grant(&mut self, sz: usize) -> Result<GrantW<N>> {
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
            let d =
                unsafe { from_raw_parts_mut(c.offset(start as isize), sz) };

            Ok(GrantW { buf: d, bbq: self.bbq })
        })
    }

    //     /// Request a writable, contiguous section of memory of up to
    //     /// `sz` bytes. If a buffer of size `sz` is not available, but
    //     /// some space (0 < available < sz) is available, then a grant
    //     /// will be given for the remaining size. If no space is available
    //     /// for writing, an error will be returned
    //     fn grant_max(&mut self, mut sz: usize) -> Result<GrantW> {
    //         // Writer component. Must never write to `read`,
    //         // be careful writing to `load`

    //         let write = self.0.write.load(Relaxed);

    //         if self.0.reserve.load(Relaxed) != write {
    //             // GRANT IN PROCESS, do not allow further grants
    //             // until the current one has been completed
    //             return Err(Error::GrantInProgress);
    //         }

    //         let read = self.0.read.load(Acquire);
    //         let max = unsafe { (*self.0.buf.as_mut_ptr()).as_mut().len() };

    //         let already_inverted = write < read;

    //         let start = if already_inverted {
    //             // In inverted case, read is always > write
    //             let remain = read - write - 1;

    //             if remain != 0 {
    //                 sz = min(remain, sz);
    //                 write
    //             } else {
    //                 // Inverted, no room is available
    //                 return Err(Error::InsufficientSize);
    //             }
    //         } else {
    //             if write != max {
    //                 // Some (or all) room remaining in un-inverted case
    //                 sz = min(max - write, sz);
    //                 write
    //             } else {
    //                 // Not inverted, but need to go inverted

    //                 // NOTE: We check read > 1, NOT read >= 1, because
    //                 // write must never == read in an inverted condition, since
    //                 // we will then not be able to tell if we are inverted or not
    //                 if read > 1 {
    //                     sz = min(read - 1, sz);
    //                     0
    //                 } else {
    //                     // Not invertible, no space
    //                     return Err(Error::InsufficientSize);
    //                 }
    //             }
    //         };

    //         // Safe write, only viewed by this task
    //         self.0.reserve.store(start + sz, Relaxed);

    //         let c = unsafe { (*self.0.buf.as_mut_ptr()).as_mut().as_mut_ptr() };
    //         let d = unsafe { from_raw_parts_mut(c, max) };

    //         Ok(GrantW {
    //             buf: &mut d[start..self.0.reserve.load(Relaxed)],
    //         })
    //     }
}

pub struct Consumer<'a, N>
where
    N: ArrayLength<u8>,
{
    bbq: NonNull<BBBuffer<N>>,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, N> Send for Consumer<'a, N>
where
    N: ArrayLength<u8>
{ }

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
    pub fn read(&mut self) -> Result<GrantR<N>> {
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

            Ok(GrantR { buf: d, bbq: self.bbq })
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
    pub fn capacity(&self) -> usize {
        N::to_usize()
    }
}

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    pub fn new() -> Self {
        Self(
            ConstBBBuffer::new(),
        )
    }
}

/// A structure representing a contiguous region of memory that
/// may be written to, and potentially "committed" to the queue
#[derive(Debug, PartialEq)]
pub struct GrantW<'a, N>
where
    N: ArrayLength<u8>
{
    buf: &'a mut [u8],
    bbq: NonNull<BBBuffer<N>>
}

/// A structure representing a contiguous region of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
#[derive(Debug, PartialEq)]
pub struct GrantR<'a, N>
where
    N: ArrayLength<u8>
{
    buf: &'a [u8],
    bbq: NonNull<BBBuffer<N>>
}

impl<'a, N> GrantW<'a, N>
where
    N: ArrayLength<u8>
{
    /// Finalizes a writable grant given by `grant()` or `grant_max()`.
    /// This makes the data available to be read via `read()`.
    ///
    /// If `used` is larger than the given grant, this function will panic.
    ///
    /// NOTE: Takes a critical section while commiting the grant.
    /// The critical section is only active for the duration of
    /// this function call.
    pub fn commit(mut self, used: usize) {
        self.commit_inner(used);
        forget(self);
    }

    #[inline(always)]
    fn commit_inner(&mut self, used: usize) {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            // Writer component. Must never write to READ,
            // be careful writing to LAST

            // Verify we are not committing more than the given
            // grant
            let len = self.buf.len();
            assert!(len >= used);

            let write = inner.write;
            inner.reserve -= len - used;

            let max = N::to_usize();
            let last = inner.last;

            // Inversion case, we have begun writing
            if (inner.reserve < write) && (write != max) {
                inner.last = write;
            } else if write > last {
                inner.last = max;
            }

            // Write must be updated AFTER last, otherwise read could think it was
            // time to invert early!
            inner.write = inner.reserve;
        })
    }
}

impl<'a, N> GrantR<'a, N>
where
    N: ArrayLength<u8>
{
    /// Release a sequence of bytes from the buffer, allowing the space
    /// to be used by later writes
    ///
    /// If `used` is larger than the given grant, this function will panic.
    ///
    /// NOTE: Takes a critical section while releasing the read grant.
    /// The critical section is only active for the duration of
    /// this function call.
    pub fn release(mut self, used: usize) {
        self.release_inner(used);
        forget(self);
    }

    #[inline(always)]
    fn release_inner(&mut self, used: usize) {
        free(|_cs| {
            let inner = unsafe { &mut self.bbq.as_mut().0 };

            assert!(used <= self.buf.len());

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
