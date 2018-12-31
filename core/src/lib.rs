#![no_std]

pub type Result<T> = ::core::result::Result<T, ()>;
use core::slice::from_raw_parts;
use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};
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
    pub fn grant(&mut self, sz: usize) -> Result<GrantW> {
        unsafe {
            self.bbq.as_mut().grant(sz)
        }
    }

    pub fn commit(&mut self, used: usize, grant: GrantW) {
        unsafe {
            self.bbq.as_mut().commit(used, grant)
        }
    }
}

impl<'a> Consumer<'a> {
    pub fn read(&mut self) -> GrantR {
        unsafe {
            self.bbq.as_mut().read()
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        unsafe {
            self.bbq.as_mut().release(used, grant)
        }
    }
}


#[derive(Debug)]
pub struct Track {
    write: AtomicUsize, // TODO ATOMIC/VOLATILE
    read: AtomicUsize,  // TODO ATOMIC/VOLATILE

    last: AtomicUsize,  // TODO ATOMIC

    reserve: usize,
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
            reserve: 0,
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

        if self.trk.reserve != self.trk.write.load(SeqCst) {
            return Err(()); // GRANT IN PROCESS
        }

        let start = if self.trk.write.load(SeqCst) + sz <= self.trk.last.load(SeqCst) {
            // Non inverted condition
            self.trk.write.load(SeqCst)
        } else {
            let read = self.trk.read.load(SeqCst);
            if (read != 0) && (sz < read) {
                // Invertable situation
                0
            } else {
                // Not invertable, no space
                return Err(());
            }
        };

        let x = &mut self.buf.split_at_mut(start).1[..sz];

        self.trk.reserve = start + sz;

        Ok(GrantW {
            buf: unsafe { ::core::mem::transmute::<_, &'static mut _>(x) },
        })
    }

    /// Writer component. Must never write to READ,
    /// be careful writing to LOAD
    pub fn commit(&mut self, used: usize, grant: GrantW) {
        let len = grant.buf.len();
        assert!(len >= used);
        drop(grant);

        self.trk.reserve -= len - used;
        // WARN
        if self.trk.reserve < self.trk.write.load(SeqCst) {
            self.trk.last.store(self.trk.write.load(SeqCst), SeqCst);
        }
        self.trk.write.store(self.trk.reserve, SeqCst);
    }

    pub fn read(&mut self) -> GrantR {
        let last = self.trk.last.load(SeqCst);
        let write = self.trk.write.load(SeqCst);

        let sz = if write < self.trk.read.load(SeqCst) {
            // Inverted, only believe last
            last
        } else {
            // Not inverted, only believe write
            write
        } - self.trk.read.load(SeqCst);

        GrantR {
            buf: unsafe { from_raw_parts(&self.buf[self.trk.read.load(SeqCst)], sz) },
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        assert!(used <= grant.buf.len());
        drop(grant);

        self.trk.read.fetch_add(used, SeqCst);

        let last = self.trk.last.load(SeqCst);
        let write = self.trk.write.load(SeqCst);

        let inverted = write < self.trk.read.load(SeqCst);

        if inverted && (self.trk.read.load(SeqCst) == last) {
            self.trk.last.store(self.buf.len(), SeqCst);
            self.trk.read.store(0, SeqCst);
        }
    }
}

// impl BBBufIn {
//     pub fn commit(self, used: usize) -> BBBufInHandle {
//         unimplemented!()
//     }

//     pub fn abort(self) {
//         // TODO, this is just drop? Needs to update upstream BBQueue
//     }
// }

// impl BBBufOut {
//     pub fn release(self) {
//         // TODO, this is just drop? Needs to update upstream BBQuque
//     }
// }
