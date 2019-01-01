// #![no_std]

pub type Result<T> = ::core::result::Result<T, ()>;
use core::slice::from_raw_parts_mut;
use core::slice::from_raw_parts;
use core::sync::atomic::{fence, AtomicUsize, Ordering::SeqCst};
use core::marker::PhantomData;
use core::ptr::NonNull;

const BONELESS_SZ: usize = 6;

#[derive(Debug)]
pub struct BBQueue {
    buf: [u8; BONELESS_SZ], // TODO, ownership et. al
    trk: Track,
}


unsafe impl<'a> Send for Producer<'a> {}
pub struct Producer<'bbq> {
    // AJM - temp debug
    pub bbq: NonNull<BBQueue>,
    ltr: PhantomData<&'bbq BBQueue>,
}

unsafe impl<'a> Send for Consumer<'a> {}
pub struct Consumer<'bbq> {
    // AJM - temp debug
    pub bbq: NonNull<BBQueue>,
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
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        unsafe {
            fence(SeqCst);
            let x = self.bbq.as_mut().grant(sz);
            fence(SeqCst);
            x
        }
    }

    pub fn commit(&mut self, used: usize, grant: GrantW) {
        unsafe {
            fence(SeqCst);
            let x = self.bbq.as_mut().commit(used, grant);
            fence(SeqCst);
            x
        }
    }
}

impl<'a> Consumer<'a> {
    pub fn read(&mut self) -> GrantR {
        unsafe {
            fence(SeqCst);
            let x = self.bbq.as_mut().read();
            fence(SeqCst);
            x
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        unsafe {
            fence(SeqCst);
            let x = self.bbq.as_mut().release(used, grant);
            fence(SeqCst);
            x
        }
    }
}


#[derive(Debug)]
pub struct Track {
    write: AtomicUsize, // TODO ATOMIC/VOLATILE
    read: AtomicUsize,  // TODO ATOMIC/VOLATILE

    last: AtomicUsize,  // TODO ATOMIC

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
            write: AtomicUsize::new(0), // TODO ATOMIC/VOLATILE

            /// Owned by the reader
            read: AtomicUsize::new(0),  // TODO ATOMIC/VOLATILE

            /// Cooperatively owned
            last: AtomicUsize::new(sz),  // TODO ATOMIC

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
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {

        // TODO check ordering
        let read = self.trk.read.load(SeqCst);
        let write = self.trk.write.load(SeqCst);
        let reserve = self.trk.reserve.load(SeqCst);
        let max = self.buf.len();

        if reserve != write {
            // GRANT IN PROCESS, do not allow further grants
            // until the current one has been completed
            return Err(());
        }

        let already_inverted = write < read;

        let start = if already_inverted {
            if (write + sz) < read {
                // Inverted, room is still available
                write
            } else {
                // Inverted, no room is available
                return Err(());
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
                    // Invertable situation
                    0
                } else {
                    // Not invertable, no space
                    return Err(());
                }
            }
        };

        debug_assert!(start < self.buf.len(), "Bad WR Grant!");

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

        debug_assert!(read < max, "Bad RD Grant!, rd: {} lt: {}", read, last);

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
