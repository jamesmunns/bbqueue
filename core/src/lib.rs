//! # BBQueue
//!
//! BBQueue, short for "BipBuffer Queue", is a (work in progress) Single Producer Single Consumer,
//! lockless, no_std, thread safe, queue, based on [BipBuffers].
//!
//! [BipBuffers]: https://www.codeproject.com/Articles/3479/%2FArticles%2F3479%2FThe-Bip-Buffer-The-Circular-Buffer-with-a-Twist
//!
//! It is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
//! systems.
//!
//! While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
//! main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
//! block of contiguous memory, which can be filled (or emptied) by a DMA engine.
//!
//! ## Using in a single threaded context
//!
//! ```rust
//! use bbqueue::{BBQueue, bbq};
//!
//! fn main() {
//!     // Create a statically allocated instance
//!     let bbq = bbq!(1024).unwrap();
//!
//!     // Obtain a write grant of size 128 bytes
//!     let wgr = bbq.grant(128).unwrap();
//!
//!     // Fill the buffer with data
//!     wgr.buf.copy_from_slice(&[0xAFu8; 128]);
//!
//!     // Commit the write, to make the data available to be read
//!     bbq.commit(wgr.buf.len(), wgr);
//!
//!     // Obtain a read grant of all available and contiguous bytes
//!     let rgr = bbq.read().unwrap();
//!
//!     for i in 0..128 {
//!         assert_eq!(rgr.buf[i], 0xAFu8);
//!     }
//!
//!     // Release the bytes, allowing the space
//!     // to be re-used for writing
//!     bbq.release(rgr.buf.len(), rgr);
//! }
//! ```
//!
//! ## Using in a multi-threaded environment (or with interrupts, etc.)
//!
//! ```rust
//! use bbqueue::{BBQueue, bbq};
//! use std::thread::spawn;
//!
//! fn main() {
//!     // Create a statically allocated instance
//!     let bbq = bbq!(1024).unwrap();
//!     let (mut tx, mut rx) = bbq.split();
//!
//!     let txt = spawn(move || {
//!         for tx_i in 0..128 {
//!             'inner: loop {
//!                 match tx.grant(4) {
//!                     Ok(gr) => {
//!                         gr.buf.copy_from_slice(&[tx_i as u8; 4]);
//!                         tx.commit(4, gr);
//!                         break 'inner;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!         }
//!     });
//!
//!     let rxt = spawn(move || {
//!         for rx_i in 0..128 {
//!             'inner: loop {
//!                 match rx.read() {
//!                     Ok(gr) => {
//!                         if gr.buf.len() < 4 {
//!                             rx.release(0, gr);
//!                             continue 'inner;
//!                         }
//!
//!                         assert_eq!(&gr.buf[..4], &[rx_i as u8; 4]);
//!                         rx.release(4, gr);
//!                         break 'inner;
//!                     }
//!                     _ => {}
//!                 }
//!             }
//!         }
//!     });
//!
//!     txt.join().unwrap();
//!     rxt.join().unwrap();
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(unused)]

use core::cmp::min;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::result::Result as CoreResult;
use core::slice::from_raw_parts;
use core::slice::from_raw_parts_mut;
use core::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering::{
        Acquire,
        Relaxed,
        Release,
    },
};
use core::cell::UnsafeCell;

/// Result type used by the `BBQueue` interfaces
pub type Result<T> = CoreResult<T, Error>;

/// Error type used by the `BBQueue` interfaces
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Error {
    InsufficientSize,
    GrantInProgress,
}

/// A single producer, single consumer, thread safe queue
#[derive(Debug)]
pub struct BBQueue {
    // Underlying data storage
    buf: NonNull<[u8]>,

    // These ZSTs are used to tie the lifetime of the Producer and Consumer
    //   back to the original structure. We have two tokens, so we can simultaneously
    //   hand out two references to the "parent" structure
    prod_token: (),
    cons_token: (),

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

impl BBQueue {
    /// Create a new BBQueue with a given backing buffer. After giving this
    /// buffer to `BBQueue::new(), the backing buffer must not be used, or
    /// undefined behavior could occur!
    ///
    /// Consider using the `bbq!()` macro to safely create a statically
    /// allocated instance, or enable the "std" feature, and instead use the
    /// `BBQueue::new_boxed()` constructor.
    pub fn new(buf: &'static mut [u8]) -> Self {
        let sz = buf.len();
        assert!(sz != 0);
        BBQueue {
            buf: unsafe { NonNull::new_unchecked(buf) },
            cons_token: (),
            prod_token: (),

            /// Owned by the writer
            write: AtomicUsize::new(0),

            /// Owned by the reader
            read: AtomicUsize::new(0),

            /// Cooperatively owned
            last: AtomicUsize::new(sz),

            /// Owned by the Writer, "private"
            reserve: AtomicUsize::new(0),

            /// Owned by the Reader, "private"
            read_in_progress: AtomicBool::new(false),

            already_split: AtomicBool::new(false),
        }
    }

    /// Request a writable, contiguous section of memory of exactly
    /// `sz` bytes. If the buffer size requested is not available,
    /// an error will be returned.
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        // Writer component. Must never write to `read`,
        // be careful writing to `load`

        let write = self.write.load(Relaxed);

        if self.reserve.load(Relaxed) != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(Error::GrantInProgress);
        }

        let read = self.read.load(Acquire);
        let max = unsafe { self.buf.as_mut().len() };

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
        self.reserve.store(start + sz, Relaxed);

        let c = unsafe { self.buf.as_mut().as_mut_ptr() };
        let d = unsafe { from_raw_parts_mut(c, max) };

        Ok(GrantW {
            buf: &mut d[start..self.reserve.load(Relaxed)],
            internal: (),
        })
    }

    /// Request a writable, contiguous section of memory of up to
    /// `sz` bytes. If a buffer of size `sz` is not available, but
    /// some space (0 < available < sz) is available, then a grant
    /// will be given for the remaining size. If no space is available
    /// for writing, an error will be returned
    pub fn grant_max(&mut self, mut sz: usize) -> Result<GrantW> {
        // Writer component. Must never write to `read`,
        // be careful writing to `load`

        let write = self.write.load(Relaxed);

        if self.reserve.load(Relaxed) != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(Error::GrantInProgress);
        }

        let read = self.read.load(Acquire);
        let max = unsafe { self.buf.as_mut().len() };

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

                // NOTE: We check read > 1, NOT read > 1, because
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
        self.reserve.store(start + sz, Relaxed);

        let c = unsafe { self.buf.as_mut().as_mut_ptr() };
        let d = unsafe { from_raw_parts_mut(c, max) };


        Ok(GrantW {
            buf: &mut d[start..self.reserve.load(Relaxed)],
            internal: (),
        })
    }

    /// Finalizes a writable grant given by `grant()` or `grant_max()`.
    /// This makes the data available to be read via `read()`.
    ///
    /// If `used` is larger than the given grant, this function will panic.
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        // Writer component. Must never write to READ,
        // be careful writing to LAST

        // Verify we are not committing more than the given
        // grant
        let len = grant.buf.len();
        assert!(len >= used);
        drop(grant);

        let write = self.write.load(Relaxed);
        self.reserve.fetch_sub(len - used, Relaxed);

        // Inversion case, we have begun writing
        if (self.reserve.load(Relaxed) < write) && (write != unsafe { self.buf.as_mut().len() }) {
            // This has potential for danger. We have two writers!
            // MOVING LAST BACKWARDS
            self.last.store(write, Release);
        }

        // This has some potential for danger. The other thread (READ)
        // does look at this variable!
        // MOVING WRITE FORWARDS
        self.write.store(self.reserve.load(Relaxed), Release);
    }

    /// Obtains a contiguous slice of committed bytes. This slice may not
    /// contain ALL available bytes, if the writer has wrapped around. The
    /// remaining bytes will be available after all readable bytes are
    /// released
    pub fn read(&mut self) -> Result<GrantR> {
        if self.read_in_progress.load(Relaxed) {
            return Err(Error::GrantInProgress);
        }

        let write = self.write.load(Acquire);
        let mut last = self.last.load(Acquire);
        let mut read = self.read.load(Relaxed);
        let max = unsafe { self.buf.as_mut().len() };

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
            self.read.store(0, Release);
            if last != max {
                // This is pretty tricky, we have two writers!
                // MOVING LAST FORWARDS
                self.last.store(max, Release);
                last = max;
            }
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

        self.read_in_progress.store(true, Relaxed);

        let c = unsafe { self.buf.as_mut().as_ptr() };
        let d = unsafe { from_raw_parts(c, max) };

        Ok(GrantR {
            buf: &d[read..read+sz],
            internal: (),
        })
    }

    /// Release a sequence of bytes from the buffer, allowing the space
    /// to be used by later writes
    ///
    /// If `used` is larger than the given grant, this function will panic.
    pub fn release(&self, used: usize, grant: GrantR) {
        assert!(used <= grant.buf.len());
        drop(grant);

        // This should be fine, purely incrementing
        let _ = self.read.fetch_add(used, Release);

        self.read_in_progress.store(false, Relaxed);
    }
}

impl BBQueue {
    /// This method takes a `BBQueue`, and returns a set of SPSC handles
    /// that may be given to separate threads. May only be called once
    /// per `BBQueue` object, or this function will panic
    pub fn split(&self) -> (Producer, Consumer) {
        assert!(!self.already_split.swap(true, Relaxed));

        let nn1 = unsafe { NonNull::new_unchecked(self as *const _ as *mut _) };
        let nn2 = unsafe { NonNull::new_unchecked(self as *const _ as *mut _) };

        (
            Producer {
                bbq: nn1,
            },
            Consumer {
                bbq: nn2,
            },
        )
    }
}

#[cfg(feature = "std")]
impl BBQueue {
    /// Creates a Boxed `BBQueue`
    ///
    /// NOTE: This function essentially "leaks" the backing buffer
    /// (e.g. `BBQueue::new_boxed(1024)` will leak 1024 bytes). This
    /// may be changed in the future.
    pub fn new_boxed(capacity: usize) -> Box<Self> {
        let mut data: &mut [u8] = Box::leak(vec![0; capacity].into_boxed_slice());
        Box::new(Self::new(data))
    }

    /// Splits a boxed `BBQueue` into a producer and consumer
    pub fn split_box(queue: Box<Self>) -> (Producer, Consumer) {
        let self_ref = Box::leak(queue);
        Self::split(self_ref)
    }
}

/// A structure representing a contiguous region of memory that
/// may be written to, and potentially "committed" to the queue
#[derive(Debug, PartialEq)]
pub struct GrantW {
    pub buf: &'static mut [u8],

    // Zero sized type preventing external construction
    internal: (),
}

/// A structure representing a contiguous region of memory that
/// may be read from, and potentially "released" (or cleared)
/// from the queue
#[derive(Debug, PartialEq)]
pub struct GrantR {
    pub buf: &'static [u8],

    // Zero sized type preventing external construction
    internal: (),
}

unsafe impl Send for Consumer {}

/// An opaque structure, capable of reading data from the queue
pub struct Consumer {
    /// The underlying `BBQueue` object`
    bbq: NonNull<BBQueue>,
}

unsafe impl Send for Producer {}

/// An opaque structure, capable of writing data to the queue
pub struct Producer {
    /// The underlying `BBQueue` object`
    bbq: NonNull<BBQueue>,
}

impl Producer {
    /// Request a writable, contiguous section of memory of exactly
    /// `sz` bytes. If the buffer size requested is not available,
    /// an error will be returned.
    #[inline(always)]
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        unsafe { self.bbq.as_mut().grant(sz) }
    }

    /// Request a writable, contiguous section of memory of up to
    /// `sz` bytes. If a buffer of size `sz` is not available, but
    /// some space (0 < available < sz) is available, then a grant
    /// will be given for the remaining size. If no space is available
    /// for writing, an error will be returned
    #[inline(always)]
    pub fn grant_max(&mut self, sz: usize) -> Result<GrantW> {
        unsafe { self.bbq.as_mut().grant_max(sz) }
    }

    /// Finalizes a writable grant given by `grant()` or `grant_max()`.
    /// This makes the data available to be read via `read()`.
    ///
    /// If `used` is larger than the given grant, this function will panic.
    #[inline(always)]
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        unsafe { self.bbq.as_mut().commit(used, grant) }
    }
}

impl Consumer {
    /// Obtains a contiguous slice of committed bytes. This slice may not
    /// contain ALL available bytes, if the writer has wrapped around. The
    /// remaining bytes will be available after all readable bytes are
    /// released
    #[inline(always)]
    pub fn read(&mut self) -> Result<GrantR> {
        unsafe { self.bbq.as_mut().read() }
    }

    /// Release a sequence of bytes from the buffer, allowing the space
    /// to be used by later writes
    ///
    /// If `used` is larger than the given grant, this function will panic.
    #[inline(always)]
    pub fn release(&mut self, used: usize, grant: GrantR) {
        unsafe { self.bbq.as_mut().release(used, grant) }
    }
}

/// Statically allocate a `BBQueue` singleton object with a given size (in bytes).
///
/// This function must only ever be called once in the same place
/// per function. For example:
///
/// ```rust,ignore
/// // this creates two separate/distinct buffers, and is an acceptable usage
/// let bbq1 = bbq!(1024).unwrap(); // Returns Some(BBQueue)
/// let bbq2 = bbq!(1024).unwrap(); // Returns Some(BBQueue)
///
/// // this is a bad example, as the same instance is executed twice!
/// // On the first call, this macro will return `Some(BBQueue)`. On the
/// // second call, this macro will return `None`!
/// for _ in 0..2 {
///     let _ = bbq!(1024).unwrap();
/// }
///
/// Additionally: this macro is not intrinsically thread safe! You must ensure that
/// calls to this macro happen "atomically"/in a "critical section", or in
/// a single-threaded context. If you are using this in a cortex-m
/// microcontroller system, consider using the cortex_m_bbq!() macro instead.
#[macro_export]
macro_rules! bbq {
    ($expr:expr) => {
        unsafe {
            static mut BUFFER: [u8; $expr] = [0u8; $expr];
            static mut BBQ: Option<BBQueue> = None;

            if BBQ.is_some() {
                None
            } else {
                BBQ = Some(BBQueue::new(&mut BUFFER));
                Some(BBQ.as_mut().unwrap())
            }
        }
    }
}

/// Like the `bbq!()` macro, but wraps the initialization in a cortex-m
/// "critical section" by disabling interrupts
#[cfg_attr(feature="cortex-m", macro_export)]
macro_rules! cortex_m_bbq {
    ($expr:expr) => {
        cortex_m::interrupt::free(|_|
            $crate::bbq!($expr)
        )
    }
}
