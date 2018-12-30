pub type Result<T> = ::std::result::Result<T, ()>;
use core::slice::from_raw_parts;

#[derive(Debug)]
pub struct BBQueue {
    buf: [u8; 6], // TODO, ownership et. al
    active: Track,
    passive: Track,
}

#[derive(Debug)]
pub struct Track {
    start: usize,
    reserve: usize,
    committed: usize,
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
    fn new() -> Self {
        Track {
            start: 0,
            reserve: 0,
            committed: 0,
        }
    }
}

impl BBQueue {
    pub fn new() -> Self {
        BBQueue {
            buf: [0u8; 6],
            active: Track::new(),
            passive: Track::new(),
        }
    }

    pub fn grant<'a>(&mut self, sz: usize) -> Result<GrantW> {
        if self.active.reserve != self.active.committed {
            return Err(()); // GRANT IN PROCESS
        }

        if self.active.reserve + sz <= self.buf.len() {
            // alloc in primary
        } else if sz < self.active.start {
            // Swap, now we're using primary
            ::core::mem::swap(&mut self.active, &mut self.passive);
        } else {
            return Err(());
        }

        let x = &mut self.buf.split_at_mut(self.active.reserve).1[..sz];

        self.active.reserve += sz;

        Ok(GrantW {
            buf: unsafe { ::core::mem::transmute::<_, &'static mut _>(x) },
        })
    }

    pub fn commit(&mut self, used: usize, grant: GrantW) {
        let len = grant.buf.len();
        assert!(len >= used);
        drop(grant);
        self.active.reserve -= len - used;

        // this probably goes last for thread safety?
        self.active.committed += used;
    }

    pub fn read(&mut self) -> GrantR {
        let avail_pas = self.passive.committed - self.passive.start;
        let avail_act = self.active.committed - self.active.start;

        match (avail_pas, avail_act) {
            (p, _a) if p != 0 => {
                // slice from passive first
                GrantR {
                    buf: unsafe { from_raw_parts(&self.buf[self.passive.start], avail_pas) },
                }
            }
            (_p, a) if a != 0 => {
                // slice from active second
                GrantR {
                    buf: unsafe { from_raw_parts(&self.buf[self.active.start], avail_act) },
                }
            }
            _ => {
                // YOU GET NOTHING
                GrantR { buf: &[] }
            }
        }
    }

    pub fn release(&mut self, used: usize, grant: GrantR) {
        assert!(used <= grant.buf.len());
        drop(grant);
        let avail_pas = self.passive.committed - self.passive.start;
        let avail_act = self.active.committed - self.active.start;

        match (avail_pas, avail_act) {
            (p, _a) if p != 0 => {
                self.passive.start += used;
                if self.passive.start == self.passive.reserve {
                    self.passive = Track::new();
                }
            }
            (_p, a) if a != 0 => {
                self.active.start += used;
                if self.active.start == self.active.reserve {
                    self.active = Track::new();
                }
            }
            _ => {
                panic!("nothing to free?");
            }
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
