use crate::{
    framed::{FrameConsumer, FrameProducer},
    Error, Result,
};
use core::{
    cell::UnsafeCell,
    cmp::min,
    marker::PhantomData,
    mem::forget,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    result::Result as CoreResult,
    slice::from_raw_parts_mut,
    sync::atomic::{
        AtomicBool, AtomicUsize,
        Ordering::{AcqRel, Acquire, Release},
    },
};


pub struct OwnedBBBuffer<const N: usize> {
    hdr: BBHeader,
    storage: UnsafeCell<[u8; N]>
}

unsafe impl<const A: usize> Sync for OwnedBBBuffer<{ A }> {}

pub struct BBHeader {
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

    /// Is there an active write grant?
    write_in_progress: AtomicBool,

    /// Have we already split?
    already_split: AtomicBool,
}

// TODO(AJM): Seal this trait? Unsafe to impl?
// Do I ever need the header XOR the storage? Or should
// they always just travel together?
//   -> Yes, they do, but it's probably not worth separating them, instead just handing
//      back some combined type. They will probably always be "allocated" together.
//
// Maybe the BBGetter trait can be replaced with AsRef<BBStorage> or something?
// Also, this would probably let anyone get access to the header of bbqueue, which
// would be wildly unsafe
pub(crate) mod sealed {
    use crate::bbbuffer::BBHeader;
    use crate::bbbuffer::OwnedBBBuffer;

    pub trait BBGetter<'a> {
        type Duplicate: BBGetter<'a>;

        fn get_header(&self) -> &BBHeader;
        fn get_storage(&self) -> (*mut u8, usize);
        fn duplicate(&self) -> Self::Duplicate;
    }

    impl<'a, const N: usize> BBGetter<'a> for OwnedBBBuffer<N> {
        type Duplicate = &'a OwnedBBBuffer<N>;

        fn get_header(&self) -> &BBHeader {
            &self.hdr
        }

        fn get_storage(&self) -> (*mut u8, usize) {
            let ptr = self.storage.get().cast();
            (ptr, N)
        }

        fn duplicate(&self) -> Self::Duplicate {
            todo!()
        }
    }

    impl<'a, const N: usize> BBGetter<'a> for &'a OwnedBBBuffer<N> {
        type Duplicate = &'a OwnedBBBuffer<N>;

        fn get_header(&self) -> &BBHeader {
            &self.hdr
        }

        fn get_storage(&self) -> (*mut u8, usize) {
            let ptr = self.storage.get().cast();
            (ptr, N)
        }

        fn duplicate(&self) -> Self::Duplicate {
            todo!()
        }
    }
}
use crate::sealed::BBGetter;

/// A backing structure for a BBQueue. Can be used to create either
/// a BBQueue or a split Producer/Consumer pair
//
// NOTE: The BBQueue is generic over ANY type,
pub struct BBQueue<STO>
{
    sto: STO
}

impl<'a, STO> BBQueue<STO> {
    pub const fn new(storage: STO) -> Self {
        Self {
            sto: storage,
        }
    }
}

impl<'a, STO: BBGetter<'a>> BBQueue<STO> {
    /// Attempt to split the `BBQueue` into `Consumer` and `Producer` halves to gain access to the
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
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer
    /// let buffer: BBQueue<6> = BBQueue::new();
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
    pub fn try_split(&'a self) -> Result<(Producer<'a, STO::Duplicate>, Consumer<'a, STO::Duplicate>)> {
        if atomic::swap(&self.sto.get_header().already_split, true, AcqRel) {
            return Err(Error::AlreadySplit);
        }

        Ok((
            Producer {
                bbq: self.sto.duplicate(),
                pd: PhantomData,
            },
            Consumer {
                bbq: self.sto.duplicate(),
                pd: PhantomData,
            },
        ))
    }

    /// Attempt to split the `BBQueue` into `FrameConsumer` and `FrameProducer` halves
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
    pub fn try_split_framed(
        &'a self,
    ) -> Result<(FrameProducer<'a, STO::Duplicate>, FrameConsumer<'a, STO::Duplicate>)> {
        let (producer, consumer) = self.try_split()?;
        Ok((FrameProducer { producer }, FrameConsumer { consumer }))
    }

    /// Attempt to release the Producer and Consumer
    ///
    /// This re-initializes the buffer so it may be split in a different mode at a later
    /// time. There must be no read or write grants active, or an error will be returned.
    ///
    /// The `Producer` and `Consumer` must be from THIS `BBQueue`, or an error will
    /// be returned.
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer
    /// let buffer: BBQueue<6> = BBQueue::new();
    /// let (prod, cons) = buffer.try_split().unwrap();
    ///
    /// // Not possible to split twice
    /// assert!(buffer.try_split().is_err());
    ///
    /// // Release the producer and consumer
    /// assert!(buffer.try_release(prod, cons).is_ok());
    ///
    /// // Split the buffer in framed mode
    /// let (fprod, fcons) = buffer.try_split_framed().unwrap();
    /// # // bbqueue test shim!
    /// # }
    /// #
    /// # fn main() {
    /// # #[cfg(not(feature = "thumbv6"))]
    /// # bbqtest();
    /// # }
    /// ```
    pub fn try_release(
        &'a self,
        prod: Producer<'a, STO>,
        cons: Consumer<'a, STO>,
    ) -> CoreResult<(), (Producer<'a, STO>, Consumer<'a, STO>)> {
        // Note: Re-entrancy is not possible because we require ownership
        // of the producer and consumer, which are not cloneable. We also
        // can assume the buffer has been split, because

        // Are these our producers and consumers?
        // NOTE: Check the header rather than the data
        let our_prod = prod.bbq.get_header() as *const BBHeader == self.sto.get_header() as *const BBHeader;
        let our_cons = cons.bbq.get_header() as *const BBHeader == self.sto.get_header() as *const BBHeader;

        if !(our_prod && our_cons) {
            // Can't release, not our producer and consumer
            return Err((prod, cons));
        }

        let hdr = self.sto.get_header();

        let wr_in_progress = hdr.write_in_progress.load(Acquire);
        let rd_in_progress = hdr.read_in_progress.load(Acquire);

        if wr_in_progress || rd_in_progress {
            // Can't release, active grant(s) in progress
            return Err((prod, cons));
        }

        // Drop the producer and consumer halves
        drop(prod);
        drop(cons);

        // Re-initialize the buffer (not totally needed, but nice to do)
        hdr.write.store(0, Release);
        hdr.read.store(0, Release);
        hdr.reserve.store(0, Release);
        hdr.last.store(0, Release);

        // Mark the buffer as ready to be split
        hdr.already_split.store(false, Release);

        Ok(())
    }

    /// Attempt to release the Producer and Consumer in Framed mode
    ///
    /// This re-initializes the buffer so it may be split in a different mode at a later
    /// time. There must be no read or write grants active, or an error will be returned.
    ///
    /// The `FrameProducer` and `FrameConsumer` must be from THIS `BBQueue`, or an error
    /// will be returned.
    pub fn try_release_framed(
        &'a self,
        prod: FrameProducer<'a, STO>,
        cons: FrameConsumer<'a, STO>,
    ) -> CoreResult<(), (FrameProducer<'a, STO>, FrameConsumer<'a, STO>)> {
        self.try_release(prod.producer, cons.consumer)
            .map_err(|(producer, consumer)| {
                // Restore the wrapper types
                (FrameProducer { producer }, FrameConsumer { consumer })
            })
    }
}

impl<const N: usize> OwnedBBBuffer<{ N }> {
    /// Create a new constant inner portion of a `BBQueue`.
    ///
    /// NOTE: This is only necessary to use when creating a `BBQueue` at static
    /// scope, and is generally never used directly. This process is necessary to
    /// work around current limitations in `const fn`, and will be replaced in
    /// the future.
    ///
    /// ```rust,no_run
    /// use bbqueue_ng::BBQueue;
    ///
    /// static BUF: BBQueue<6> = BBQueue::new();
    ///
    /// fn main() {
    ///    let (prod, cons) = BUF.try_split().unwrap();
    /// }
    /// ```
    pub const fn new() -> Self {
        Self {
            // This will not be initialized until we split the buffer
            storage: UnsafeCell::new([0u8; N]),

            hdr: BBHeader {
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

                /// Owned by the Writer, "private"
                write_in_progress: AtomicBool::new(false),

                /// We haven't split at the start
                already_split: AtomicBool::new(false),
            }
        }
    }
}

/// `Producer` is the primary interface for pushing data into a `BBQueue`.
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
pub struct Producer<'a, STO>
where
    STO: BBGetter<'a>,
{
    // TODO: Is 'a the right lifetime?
    bbq: STO,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, STO> Send for Producer<'a, STO>
where
    STO: BBGetter<'a>,
{}

impl<'a, STO> Producer<'a, STO>
where
    STO: BBGetter<'a>,
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
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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
    pub fn grant_exact(&mut self, sz: usize) -> Result<GrantW<'a, STO::Duplicate>> {
        let hdr = self.bbq.get_header();
        let sto = self.bbq.get_storage();


        if atomic::swap(&hdr.write_in_progress, true, AcqRel) {
            return Err(Error::GrantInProgress);
        }

        // Writer component. Must never write to `read`,
        // be careful writing to `load`
        let write = hdr.write.load(Acquire);
        let read = hdr.read.load(Acquire);
        let max = sto.1;
        let already_inverted = write < read;

        let start = if already_inverted {
            if (write + sz) < read {
                // Inverted, room is still available
                write
            } else {
                // Inverted, no room is available
                hdr.write_in_progress.store(false, Release);
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
                    hdr.write_in_progress.store(false, Release);
                    return Err(Error::InsufficientSize);
                }
            }
        };

        // Safe write, only viewed by this task
        hdr.reserve.store(start + sz, Release);

        // This is sound, as UnsafeCell is `#[repr(Transparent)]
        // Here we are casting a `*mut [u8; N]` to a `*mut u8`
        let start_of_buf_ptr = sto.0;
        let grant_slice =
            unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(start as isize), sz) };

        Ok(GrantW {
            buf: grant_slice,
            bbq: self.bbq.duplicate(),
            to_commit: 0,
            pd: PhantomData,
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
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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
    pub fn grant_max_remaining(&mut self, mut sz: usize) -> Result<GrantW<'a, STO::Duplicate>> {
        let hdr = self.bbq.get_header();
        let sto = self.bbq.get_storage();

        if atomic::swap(&hdr.write_in_progress, true, AcqRel) {
            return Err(Error::GrantInProgress);
        }

        // Writer component. Must never write to `read`,
        // be careful writing to `load`
        let write = hdr.write.load(Acquire);
        let read = hdr.read.load(Acquire);
        let max = sto.1;

        let already_inverted = write < read;

        let start = if already_inverted {
            // In inverted case, read is always > write
            let remain = read - write - 1;

            if remain != 0 {
                sz = min(remain, sz);
                write
            } else {
                // Inverted, no room is available
                hdr.write_in_progress.store(false, Release);
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
                    hdr.write_in_progress.store(false, Release);
                    return Err(Error::InsufficientSize);
                }
            }
        };

        // Safe write, only viewed by this task
        hdr.reserve.store(start + sz, Release);

        // This is sound, as UnsafeCell is `#[repr(Transparent)]
        // Here we are casting a `*mut [u8; N]` to a `*mut u8`
        let start_of_buf_ptr = sto.0;
        let grant_slice =
            unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(start as isize), sz) };

        Ok(GrantW {
            buf: grant_slice,
            bbq: self.bbq.duplicate(),
            to_commit: 0,
            pd: PhantomData,
        })
    }
}

/// `Consumer` is the primary interface for reading data from a `BBQueue`.
pub struct Consumer<'a, STO: BBGetter<'a>> {
    // TODO: Is 'a the right lifetime?
    bbq: STO,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, STO: BBGetter<'a>> Send for Consumer<'a, STO> {}

impl<'a, STO: BBGetter<'a>> Consumer<'a, STO> {
    /// Obtains a contiguous slice of committed bytes. This slice may not
    /// contain ALL available bytes, if the writer has wrapped around. The
    /// remaining bytes will be available after all readable bytes are
    /// released
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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
    pub fn read(&mut self) -> Result<GrantR<'a, STO::Duplicate>> {
        let hdr = self.bbq.get_header();
        let sto = self.bbq.get_storage();

        if atomic::swap(&hdr.read_in_progress, true, AcqRel) {
            return Err(Error::GrantInProgress);
        }

        let write = hdr.write.load(Acquire);
        let last = hdr.last.load(Acquire);
        let mut read = hdr.read.load(Acquire);

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
            hdr.read.store(0, Release);
        }

        let sz = if write < read {
            // Inverted, only believe last
            last
        } else {
            // Not inverted, only believe write
            write
        } - read;

        if sz == 0 {
            hdr.read_in_progress.store(false, Release);
            return Err(Error::InsufficientSize);
        }

        let start_of_buf_ptr = sto.0;
        let grant_slice = unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(read as isize), sz) };

        Ok(GrantR {
            buf: grant_slice,
            bbq: self.bbq.duplicate(),
            to_release: 0,
            pd: PhantomData,
        })
    }

    /// Obtains two disjoint slices, which are each contiguous of committed bytes.
    /// Combined these contain all previously commited data.
    pub fn split_read(&mut self) -> Result<SplitGrantR<'a, STO::Duplicate>> {
        let hdr = self.bbq.get_header();
        let sto = self.bbq.get_storage();

        if atomic::swap(&hdr.read_in_progress, true, AcqRel) {
            return Err(Error::GrantInProgress);
        }

        let write = hdr.write.load(Acquire);
        let last = hdr.last.load(Acquire);
        let mut read = hdr.read.load(Acquire);

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
            hdr.read.store(0, Release);
        }

        let (sz1, sz2) = if write < read {
            // Inverted, only believe last
            (last - read, write)
        } else {
            // Not inverted, only believe write
            (write - read, 0)
        };

        if sz1 == 0 {
            hdr.read_in_progress.store(false, Release);
            return Err(Error::InsufficientSize);
        }

        // This is sound, as UnsafeCell is `#[repr(Transparent)]
        // Here we are casting a `*mut [u8; N]` to a `*mut u8`
        let start_of_buf_ptr = sto.0;
        let grant_slice1 =
            unsafe { from_raw_parts_mut(start_of_buf_ptr.offset(read as isize), sz1) };
        let grant_slice2 = unsafe { from_raw_parts_mut(start_of_buf_ptr, sz2) };

        Ok(SplitGrantR {
            buf1: grant_slice1,
            buf2: grant_slice2,
            bbq: self.bbq.duplicate(),
            to_release: 0,
            pd: PhantomData,
        })
    }
}

impl<'a, STO: BBGetter<'a>> BBQueue<STO> {
    /// Returns the size of the backing storage.
    ///
    /// This is the maximum number of bytes that can be stored in this queue.
    ///
    /// ```rust
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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
        self.sto.get_storage().1
    }
}

/// A structure representing a contiguous region of memory that
/// may be written to, and potentially "committed" to the queue.
///
/// NOTE: If the grant is dropped without explicitly commiting
/// the contents, or by setting a the number of bytes to
/// automatically be committed with `to_commit()`, then no bytes
/// will be comitted for writing.
///
/// If the `thumbv6` feature is selected, dropping the grant
/// without committing it takes a short critical section,
#[derive(Debug, PartialEq)]
pub struct GrantW<'a, STO>
where
    STO: BBGetter<'a>,
{
    pub(crate) buf: &'a mut [u8],
    // TODO: Is 'a the right lifetime?
    bbq: STO,
    pub(crate) to_commit: usize,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, STO: BBGetter<'a>> Send for GrantW<'a, STO> {}

/// A structure representing a contiguous region of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
///
/// NOTE: If the grant is dropped without explicitly releasing
/// the contents, or by setting the number of bytes to automatically
/// be released with `to_release()`, then no bytes will be released
/// as read.
///
///
/// If the `thumbv6` feature is selected, dropping the grant
/// without releasing it takes a short critical section,
#[derive(Debug, PartialEq)]
pub struct GrantR<'a, STO>
where
    STO: BBGetter<'a>,
{
    pub(crate) buf: &'a mut [u8],
    // TODO: Is 'a the right lifetime?
    bbq: STO,
    pub(crate) to_release: usize,
    pd: PhantomData<&'a ()>,
}

/// A structure representing up to two contiguous regions of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
#[derive(Debug, PartialEq)]
pub struct SplitGrantR<'a, STO>
where
    STO: BBGetter<'a>,
{
    pub(crate) buf1: &'a mut [u8],
    pub(crate) buf2: &'a mut [u8],
    // TODO: Is 'a the right lifetime?
    bbq: STO,
    pub(crate) to_release: usize,
    pd: PhantomData<&'a ()>,
}

unsafe impl<'a, STO: BBGetter<'a>> Send for GrantR<'a, STO> {}

unsafe impl<'a, STO: BBGetter<'a>> Send for SplitGrantR<'a, STO> {}

impl<'a, STO: BBGetter<'a>> GrantW<'a, STO> {
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
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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

    #[inline(always)]
    pub(crate) fn commit_inner(&mut self, used: usize) {
        let hdr = self.bbq.get_header();
        let sto = self.bbq.get_storage();

        // If there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !hdr.write_in_progress.load(Acquire) {
            return;
        }

        // Writer component. Must never write to READ,
        // be careful writing to LAST

        // Saturate the grant commit
        let len = self.buf.len();
        let used = min(len, used);

        let write = hdr.write.load(Acquire);
        atomic::fetch_sub(&hdr.reserve, len - used, AcqRel);

        let max = sto.1;
        let last = hdr.last.load(Acquire);
        let new_write = hdr.reserve.load(Acquire);

        if (new_write < write) && (write != max) {
            // We have already wrapped, but we are skipping some bytes at the end of the ring.
            // Mark `last` where the write pointer used to be to hold the line here
            hdr.last.store(write, Release);
        } else if new_write > last {
            // We're about to pass the last pointer, which was previously the artificial
            // end of the ring. Now that we've passed it, we can "unlock" the section
            // that was previously skipped.
            //
            // Since new_write is strictly larger than last, it is safe to move this as
            // the other thread will still be halted by the (about to be updated) write
            // value
            hdr.last.store(max, Release);
        }
        // else: If new_write == last, either:
        // * last == max, so no need to write, OR
        // * If we write in the end chunk again, we'll update last to max next time
        // * If we write to the start chunk in a wrap, we'll update last when we
        //     move write backwards

        // Write must be updated AFTER last, otherwise read could think it was
        // time to invert early!
        hdr.write.store(new_write, Release);

        // Allow subsequent grants
        hdr.write_in_progress.store(false, Release);
    }

    /// Configures the amount of bytes to be commited on drop.
    pub fn to_commit(&mut self, amt: usize) {
        self.to_commit = self.buf.len().min(amt);
    }
}

impl<'a, STO: BBGetter<'a>> GrantR<'a, STO> {
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
        let mut new_buf: &mut [u8] = &mut [];
        core::mem::swap(&mut self.buf, &mut new_buf);
        let (new, _) = new_buf.split_at_mut(len);
        self.buf = new;
    }

    /// Obtain access to the inner buffer for reading
    ///
    /// ```
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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

    /// Obtain mutable access to the read grant
    ///
    /// This is useful if you are performing in-place operations
    /// on an incoming packet, such as decryption
    pub fn buf_mut(&mut self) -> &mut [u8] {
        self.buf
    }

    #[inline(always)]
    pub(crate) fn release_inner(&mut self, used: usize) {
        let hdr = self.bbq.get_header();

        // If there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !hdr.read_in_progress.load(Acquire) {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.buf.len());

        // This should be fine, purely incrementing
        let _ = atomic::fetch_add(&hdr.read, used, Release);

        hdr.read_in_progress.store(false, Release);
    }

    /// Configures the amount of bytes to be released on drop.
    pub fn to_release(&mut self, amt: usize) {
        self.to_release = self.buf.len().min(amt);
    }
}

impl<'a, STO: BBGetter<'a>> SplitGrantR<'a, STO> {
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
        let used = min(self.combined_len(), used);

        self.release_inner(used);
        forget(self);
    }

    /// Obtain access to both inner buffers for reading
    ///
    /// ```
    /// # // bbqueue test shim!
    /// # fn bbqtest() {
    /// use bbqueue_ng::BBQueue;
    ///
    /// // Create and split a new buffer of 6 elements
    /// let buffer: BBQueue<6> = BBQueue::new();
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
    pub fn bufs(&self) -> (&[u8], &[u8]) {
        (self.buf1, self.buf2)
    }

    /// Obtain mutable access to both parts of the read grant
    ///
    /// This is useful if you are performing in-place operations
    /// on an incoming packet, such as decryption
    pub fn bufs_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        (self.buf1, self.buf2)
    }

    #[inline(always)]
    pub(crate) fn release_inner(&mut self, used: usize) {
        let hdr = self.bbq.get_header();

        // If there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !hdr.read_in_progress.load(Acquire) {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.combined_len());

        if used <= self.buf1.len() {
            // This should be fine, purely incrementing
            let _ = atomic::fetch_add(&hdr.read, used, Release);
        } else {
            // Also release parts of the second buffer
            hdr.read.store(used - self.buf1.len(), Release);
        }

        hdr.read_in_progress.store(false, Release);
    }

    /// Configures the amount of bytes to be released on drop.
    pub fn to_release(&mut self, amt: usize) {
        self.to_release = self.combined_len().min(amt);
    }

    /// The combined length of both buffers
    pub fn combined_len(&self) -> usize {
        self.buf1.len() + self.buf2.len()
    }
}

impl<'a, STO> Drop for GrantW<'a, STO>
where
    STO: BBGetter<'a>,
{
    fn drop(&mut self) {
        self.commit_inner(self.to_commit)
    }
}

impl<'a, STO> Drop for GrantR<'a, STO>
where
    STO: BBGetter<'a>,
{
    fn drop(&mut self) {
        self.release_inner(self.to_release)
    }
}

impl<'a, STO> Deref for GrantW<'a, STO>
where
    STO: BBGetter<'a>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

impl<'a, STO> DerefMut for GrantW<'a, STO>
where
    STO: BBGetter<'a>,
{
    fn deref_mut(&mut self) -> &mut [u8] {
        self.buf
    }
}

impl<'a, STO> Deref for GrantR<'a, STO>
where
    STO: BBGetter<'a>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.buf
    }
}

impl<'a, STO> DerefMut for GrantR<'a, STO>
where
    STO: BBGetter<'a>,
{
    fn deref_mut(&mut self) -> &mut [u8] {
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
