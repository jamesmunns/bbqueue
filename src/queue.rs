use core::marker::PhantomData;

use crate::{
    prod_cons::{
        framed::{FramedConsumer, FramedProducer},
        stream::{StreamConsumer, StreamProducer},
    },
    traits::{
        coordination::Coord,
        notifier::Notifier,
        storage::{ConstStorage, Storage},
    },
};

#[cfg(feature = "std")]
use crate::traits::bbqhdl::BbqHandle;

/// A standard bbqueue
pub struct BBQueue<S, C, N> {
    pub(crate) sto: S,
    pub(crate) cor: C,
    pub(crate) not: N,
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self {
            sto,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

/// A BBQueue wrapped in an Arc
#[cfg(feature = "std")]
pub struct ArcBBQueue<S, C, N>(pub(crate) std::sync::Arc<BBQueue<S, C, N>>);

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> ArcBBQueue<S, C, N> {
    pub fn new_with_storage(sto: S) -> Self {
        Self(std::sync::Arc::new(BBQueue::new_with_storage(sto)))
    }
}

#[allow(clippy::new_without_default)]
impl<S: ConstStorage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    pub const fn framed_producer(&self) -> FramedProducer<&'_ Self> {
        FramedProducer {
            bbq: self,
            pd: PhantomData,
        }
    }

    pub const fn framed_consumer(&self) -> FramedConsumer<&'_ Self> {
        FramedConsumer {
            bbq: self,
            pd: PhantomData,
        }
    }

    pub const fn stream_producer(&self) -> StreamProducer<&'_ Self> {
        StreamProducer { bbq: self }
    }

    pub const fn stream_consumer(&self) -> StreamConsumer<&'_ Self> {
        StreamConsumer { bbq: self }
    }
}

#[cfg(feature = "std")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    pub fn framed_producer(&self) -> FramedProducer<std::sync::Arc<BBQueue<S, C, N>>> {
        FramedProducer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn framed_consumer(&self) -> FramedConsumer<std::sync::Arc<BBQueue<S, C, N>>> {
        FramedConsumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    pub fn stream_producer(&self) -> StreamProducer<std::sync::Arc<BBQueue<S, C, N>>> {
        StreamProducer {
            bbq: self.0.bbq_ref(),
        }
    }

    pub fn stream_consumer(&self) -> StreamConsumer<std::sync::Arc<BBQueue<S, C, N>>> {
        StreamConsumer {
            bbq: self.0.bbq_ref(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::traits::{
        coordination::cas::AtomicCoord, notifier::blocking::Blocking, storage::Inline,
    };

    use super::*;

    type Queue = BBQueue<Inline<4096>, AtomicCoord, Blocking>;
    static QUEUE: Queue = BBQueue::new();
    static PRODUCER: FramedProducer<&'static Queue, u16> = QUEUE.framed_producer();
    static CONSUMER: FramedConsumer<&'static Queue, u16> = QUEUE.framed_consumer();

    #[test]
    fn handles() {
        let mut wgr = PRODUCER.grant(16).unwrap();
        wgr.iter_mut().for_each(|w| *w = 123);
        wgr.commit(16);

        let rgr = CONSUMER.read().unwrap();
        assert_eq!(rgr.len(), 16);
        for b in rgr.iter() {
            assert_eq!(*b, 123);
        }
        rgr.release();
    }
}
