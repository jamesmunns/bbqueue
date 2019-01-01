#![no_std]

use core::slice::from_raw_parts_mut;
use core::slice::from_raw_parts;
use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::result::Result as CoreResult;

pub type Result<T> = CoreResult<T, Error>;

#[derive(Debug)]
pub enum Error {
    InsufficientSize,
    GrantInProgress,
}

const BONELESS_SZ: usize = 6;

#[derive(Debug)]
pub struct BBQueue {
    buf: [u8; BONELESS_SZ], // TODO, ownership et. al
    trk: Track,
}


unsafe impl<'a> Send for Producer<'a> {}
pub struct Producer<'bbq> {
    bbq: NonNull<BBQueue>,
    ltr: PhantomData<&'bbq BBQueue>,
}

unsafe impl<'a> Send for Consumer<'a> {}
pub struct Consumer<'bbq> {
    bbq: NonNull<BBQueue>,
    ltr: PhantomData<&'bbq BBQueue>,
}

impl BBQueue {
    pub fn split<'bbq>(&'bbq mut self) -> (Producer<'bbq>, Consumer<'bbq>) {
        (
            Producer {
                bbq: unsafe { NonNull::new_unchecked(self) },
                ltr: PhantomData,
            },
            Consumer {
                bbq: unsafe { NonNull::new_unchecked(self) },
                ltr: PhantomData,
            }
        )
    }
}

impl<'a> Producer<'a> {
    #[inline(always)]
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        unsafe {
            self.bbq.as_mut().grant(sz)
        }
    }

    #[inline(always)]
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        unsafe {
            self.bbq.as_mut().commit(used, grant)
        }
    }
}

impl<'a> Consumer<'a> {
    #[inline(always)]
    pub fn read(&mut self) -> GrantR {
        unsafe {
            self.bbq.as_mut().read()
        }
    }

    #[inline(always)]
    pub fn release(&mut self, used: usize, grant: GrantR) {
        unsafe {
            self.bbq.as_mut().release(used, grant)
        }
    }
}


#[derive(Debug)]
pub struct Track {
    /// Where the next byte will be written
    write: AtomicUsize,

    /// Where the next byte will be read from
    read: AtomicUsize,

    /// Used in the inverted case to mark the end of the
    /// readable streak. Otherwise will == self.buf.len().
    /// Writer is responsible for placing this at the correct
    /// place when entering an inverted condition, and Reader
    /// is responsible for moving it back to self.buf.len()
    /// when exiting the inverted condition
    last: AtomicUsize,

    /// Used by the Writer to remember what bytes are currently
    /// allowed to be written to, but are not yet ready to be
    /// read from
    reserve: AtomicUsize,
}

#[derive(Debug)]
pub struct GrantW {
    // TODO, how to tie this to the lifetime of BBQueue?
    pub buf: &'static mut [u8],
}

#[derive(Debug)]
pub struct GrantR {
    // TODO, how to tie this to the lifetime of BBQueue?
    pub buf: &'static [u8],
}

impl Track {
    fn new(sz: usize) -> Self {
        Track {
            /// Owned by the writer
            write: AtomicUsize::new(0),

            /// Owned by the reader
            read: AtomicUsize::new(0),

            /// Cooperatively owned
            last: AtomicUsize::new(sz),

            /// Owned by the Writer, "private"
            reserve: AtomicUsize::new(0), // AJM - is this necessary?
        }
    }
}

impl BBQueue {
    pub fn new() -> Self {
        BBQueue {
            buf: [0u8; BONELESS_SZ],
            trk: Track::new(BONELESS_SZ),
        }
    }

    /// Writer component. Must never write to `read`,
    /// be careful writing to `load`
    ///
    /// TODO: interface that allows grant of maximum remaining
    /// size?
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        // Load all items first. Order matters here!
        let read = self.trk.read.load(SeqCst);
        let write = self.trk.write.load(SeqCst);
        let reserve = self.trk.reserve.load(SeqCst);
        let max = self.buf.len();

        if reserve != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(Error::GrantInProgress);
        }

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

        self.trk.reserve.store(start + sz, SeqCst);

        Ok(GrantW {
            buf: unsafe { from_raw_parts_mut(&mut self.buf[start], sz) },
        })
    }

    /// Writer component. Must never write to READ,
    /// be careful writing to LOAD
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        let len = grant.buf.len();
        assert!(len >= used);
        drop(grant);

        let write = self.trk.write.load(SeqCst);
        let old_reserve = self.trk.reserve.load(SeqCst);
        let new_reserve = old_reserve - (len - used);

        self.trk.reserve.store(new_reserve, SeqCst);
        // Inversion case, we have begun writing
        if new_reserve < write {
            self.trk.last.store(write, SeqCst);
        }
        self.trk.write.store(new_reserve, SeqCst);
    }

    pub fn read(&mut self) -> GrantR {
        let mut last = self.trk.last.load(SeqCst);
        let write = self.trk.write.load(SeqCst);
        let mut read = self.trk.read.load(SeqCst);
        let max = self.buf.len();

        // Resolve the inverted case or end of read
        if read == last {
            self.trk.read.store(0, SeqCst);
            self.trk.last.store(max, SeqCst);
            read = 0;
            last = max;
        }

        let sz = if write < read {
            // Inverted, only believe last
            last
        } else {
            // Not inverted, only believe write
            write
        } - read;

        GrantR {
            buf: unsafe { from_raw_parts(&self.buf[read], sz) },
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        assert!(used <= grant.buf.len());
        drop(grant);
        let _ = self.trk.read.fetch_add(used, SeqCst);
    }
}
