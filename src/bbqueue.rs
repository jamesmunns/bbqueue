pub type Result<T> = ::std::result::Result<T, ()>;
use core::slice::from_raw_parts;

#[derive(Debug)]
pub struct BBQueue {
    buf: [u8; 6], // TODO, ownership et. al
    trk: Track,
}

#[derive(Debug)]
pub struct Track {
    write: usize, // TODO ATOMIC/VOLATILE
    read: usize,  // TODO ATOMIC/VOLATILE

    last: usize,  // TODO ATOMIC

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
            write: 0, // TODO ATOMIC/VOLATILE

            /// Owned by the reader
            read: 0,  // TODO ATOMIC/VOLATILE

            /// Cooperatively owned
            last: sz,  // TODO ATOMIC

            /// Owned by the Writer, "private"
            reserve: 0,
        }
    }
}

impl BBQueue {
    pub fn new() -> Self {
        BBQueue {
            buf: [0u8; 6],
            trk: Track::new(6),
        }
    }

    /// Writer component. Must never write to `read`,
    /// be careful writing to `load`
    pub fn grant<'a>(&mut self, sz: usize) -> Result<GrantW> {

        if self.trk.reserve != self.trk.write {
            return Err(()); // GRANT IN PROCESS
        }

        let start = if self.trk.write + sz <= self.trk.last {
            // Non inverted condition
            self.trk.write
        } else {
            let read = self.trk.read;
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
        if self.trk.reserve < self.trk.write {
            self.trk.last = self.trk.write;
        }
        self.trk.write = self.trk.reserve;
    }

    pub fn read(&mut self) -> GrantR {
        let last = self.trk.last;
        let write = self.trk.write;

        let sz = if write < self.trk.read {
            // Inverted, only believe last
            last
        } else {
            // Not inverted, only believe write
            write
        } - self.trk.read;

        GrantR {
            buf: unsafe { from_raw_parts(&self.buf[self.trk.read], sz) },
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        assert!(used <= grant.buf.len());
        drop(grant);

        self.trk.read += used;

        let last = self.trk.last;
        let write = self.trk.write;

        let inverted = write < self.trk.read;

        if inverted && (self.trk.read == last) {
            self.trk.last = self.buf.len();
            self.trk.read = 0;
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
