type Result<T> = ::std::result::Result<T, ()>;

struct BBQueue {
    buf: [u8; 32], // TODO, ownership et. al
    active: Track,
    passive: Track,
}

struct BBBufIn {
    buf: &'static [u8]
}

struct BBBufInHandle {

}

struct BBBufOut {

}

struct Track {
    start: usize,
    reserve: usize,
    committed: usize
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
            buf: [0u8; 32],
            active: Track::new(),
            passive: Track::new(),
        }
    }

    pub fn grant(&mut self, size: usize) -> Result<BBBufIn> {
        if self.active.reserve + size < self.buf.len() {
            // alloc in primary
        } else if size < self.active.start {
            // Swap, now we're using primary
            ::core::mem::swap(
                &mut self.active,
                &mut self.passive
            );
        } else {
            return Err(());
        }

        self.active.start += size;

        let x = BBBufIn {
            buf: unsafe {
                ::core::slice::from_raw_parts(
                    &self.buf[self.active.start],
                    size
                )
            },
        };

        Ok(x)
    }


}

impl BBBufIn {
    pub fn commit(self, used: usize) -> BBBufInHandle {
        unimplemented!()
    }

    pub fn abort(self) {
        // TODO, this is just drop? Needs to update upstream BBQueue
    }
}

impl BBBufOut {
    pub fn release(self) {
        // TODO, this is just drop? Needs to update upstream BBQuque
    }
}
