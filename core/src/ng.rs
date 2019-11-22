use crate::{Error, GrantR, GrantW, Result};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::{size_of, MaybeUninit};
use core::ptr::NonNull;
use core::slice::from_raw_parts;
use core::slice::from_raw_parts_mut;
use core::sync::atomic::{
    AtomicBool, AtomicUsize,
    Ordering::{Acquire, Relaxed, Release},
};
use generic_array::{ArrayLength, GenericArray};

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
pub struct BBBuffer<N: ArrayLength<u8>> {
    // Underlying data storage
    inner: ConstBBBuffer<GenericArray<u8, N>>,
}

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    pub fn try_split(&self) -> Result<(Producer<N>, Consumer<N>)> {
        if self.inner.already_split.swap(true, Relaxed) {
            return Err(Error::Refactoring);
        } else {
            unsafe {
                // Explicitly zero the data to avoid undefined behavior.
                // This is required, because we hand out references to the buffers,
                // which mean that creating them as references is technically UB for now
                let mu_ptr = self.inner.buf.get();
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
}

/// `const-fn` version BBBuffer
pub struct ConstBBBuffer<A> {
    buf: UnsafeCell<MaybeUninit<A>>,

    /// Where the next byte will be written
    write: AtomicUsize,

    /// Where the next byte will be read from
    read: AtomicUsize,

    /// Used in the inverted case to mark the end of the
    /// readable streak. Otherwise will == unsafe { self.buf.as_mut().len() }.
    /// Writer is responsible for placing this at the correct
    /// place when entering an inverted condition, and Reader
    /// is responsible for moving it back to unsafe { self.buf.as_mut().len() }
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
    pub const fn new() -> Self {
        Self {
            // Note, we contain u8's, so zeroing is a sound strategy for
            // initialization
            buf: UnsafeCell::new(MaybeUninit::uninit()),

            /// Owned by the writer
            write: AtomicUsize::new(0),

            /// Owned by the reader
            read: AtomicUsize::new(0),

            /// Cooperatively owned
            last: AtomicUsize::new(size_of::<A>()),

            /// Owned by the Writer, "private"
            reserve: AtomicUsize::new(0),

            /// Owned by the Reader, "private"
            read_in_progress: AtomicBool::new(false),

            already_split: AtomicBool::new(false),
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

impl<'a, N> Producer<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Request a writable, contiguous section of memory of exactly
    /// `sz` bytes. If the buffer size requested is not available,
    /// an error will be returned.
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        let inner = unsafe { &self.bbq.as_ref().inner };

        // Writer component. Must never write to `read`,
        // be careful writing to `load`
        let write = inner.write.load(Relaxed);

        if inner.reserve.load(Relaxed) != write {
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
        inner.reserve.store(start + sz, Relaxed);

        let c = unsafe { (*inner.buf.get()).as_mut_ptr().cast::<u8>() };
        let d = unsafe { from_raw_parts_mut(c.offset(start as isize), inner.reserve.load(Relaxed)) };

        Ok(GrantW {
            buf: d,
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

    //         let write = self.inner.write.load(Relaxed);

    //         if self.inner.reserve.load(Relaxed) != write {
    //             // GRANT IN PROCESS, do not allow further grants
    //             // until the current one has been completed
    //             return Err(Error::GrantInProgress);
    //         }

    //         let read = self.inner.read.load(Acquire);
    //         let max = unsafe { (*self.inner.buf.as_mut_ptr()).as_mut().len() };

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
    //         self.inner.reserve.store(start + sz, Relaxed);

    //         let c = unsafe { (*self.inner.buf.as_mut_ptr()).as_mut().as_mut_ptr() };
    //         let d = unsafe { from_raw_parts_mut(c, max) };

    //         Ok(GrantW {
    //             buf: &mut d[start..self.inner.reserve.load(Relaxed)],
    //         })
    //     }


    /// Finalizes a writable grant given by `grant()` or `grant_max()`.
    /// This makes the data available to be read via `read()`.
    ///
    /// If `used` is larger than the given grant, this function will panic.
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        let inner = unsafe { &self.bbq.as_ref().inner };

        // Writer component. Must never write to READ,
        // be careful writing to LAST

        // Verify we are not committing more than the given
        // grant
        let len = grant.buf.len();
        assert!(len >= used);

        // Verify we are committing OUR grant
        assert!(unsafe { self.bbq.as_ref().is_our_grant(&grant.buf) });

        drop(grant);

        let write = inner.write.load(Relaxed);
        inner.reserve.fetch_sub(len - used, Relaxed);

        let max = N::to_usize();
        let last = inner.last.load(Relaxed);

        // Inversion case, we have begun writing
        if (inner.reserve.load(Relaxed) < write) && (write != max) {
            inner.last.store(write, Release);
        } else if write > last {
            inner.last.store(max, Release);
        }

        // Write must be updated AFTER last, otherwise read could think it was
        // time to invert early!
        inner.write.store(inner.reserve.load(Relaxed), Release);
    }
}

pub struct Consumer<'a, N>
where
    N: ArrayLength<u8>,
{
    bbq: NonNull<BBBuffer<N>>,
    pd: PhantomData<&'a ()>,
}

impl<'a, N> Consumer<'a, N>
where
    N: ArrayLength<u8>,
{

    /// Obtains a contiguous slice of committed bytes. This slice may not
    /// contain ALL available bytes, if the writer has wrapped around. The
    /// remaining bytes will be available after all readable bytes are
    /// released
    pub fn read(&mut self) -> Result<GrantR> {
        let inner = unsafe { &self.bbq.as_ref().inner };

        if inner.read_in_progress.load(Relaxed) {
            return Err(Error::GrantInProgress);
        }

        let write = inner.write.load(Acquire);
        let last = inner.last.load(Acquire);
        let mut read = inner.read.load(Relaxed);

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

        inner.read_in_progress.store(true, Relaxed);

        let c = unsafe { (*inner.buf.get()).as_ptr().cast::<u8>() };
        let d = unsafe { from_raw_parts(c.offset(read as isize), sz) };

        Ok(GrantR {
            buf: d,
        })
    }

    /// Release a sequence of bytes from the buffer, allowing the space
    /// to be used by later writes
    ///
    /// If `used` is larger than the given grant, this function will panic.
    pub fn release(&mut self, used: usize, grant: GrantR) {
        let inner = unsafe { &self.bbq.as_ref().inner };

        assert!(used <= grant.buf.len());

        // Verify we are committing OUR grant
        assert!(unsafe { self.bbq.as_ref().is_our_grant(&grant.buf) });

        drop(grant);

        // This should be fine, purely incrementing
        let _ = inner.read.fetch_add(used, Release);

        inner.read_in_progress.store(false, Relaxed);
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


    pub(crate) fn is_our_grant(&self, gr_buf: &[u8]) -> bool {
        let buf_start = self.inner.buf.get() as usize;
        let gr_start = gr_buf.as_ptr() as usize;
        let buf_end_plus_one = buf_start + N::to_usize();
        let gr_end_plus_one = gr_start + gr_buf.len();

        (buf_start <= gr_start) && (gr_end_plus_one <= buf_end_plus_one)
    }
}

impl<N> BBBuffer<N>
where
    N: ArrayLength<u8>,
{
    pub fn new() -> Self {
        Self {
            inner: ConstBBBuffer::new(),
        }
    }
}
