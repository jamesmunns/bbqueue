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

#[cfg(feature = "alloc")]
use crate::traits::bbqhdl::BbqHandle;

/// A standard bbqueue
pub struct BBQueue<S, C, N> {
    pub(crate) sto: S,
    pub(crate) cor: C,
    pub(crate) not: N,
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new [`BBQueue`] with the given [`Storage`] impl
    pub fn new_with_storage(sto: S) -> Self {
        Self {
            sto,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

/// A BBQueue wrapped in an Arc
#[cfg(feature = "alloc")]
pub struct ArcBBQueue<S, C, N>(pub(crate) alloc::sync::Arc<BBQueue<S, C, N>>);

#[cfg(feature = "alloc")]
impl<S: Storage, C: Coord, N: Notifier> ArcBBQueue<S, C, N> {
    /// Create a new [`BBQueue`] with the given [`Storage`] impl
    pub fn new_with_storage(sto: S) -> Self {
        Self(alloc::sync::Arc::new(BBQueue::new_with_storage(sto)))
    }
}

#[allow(clippy::new_without_default)]
impl<S: ConstStorage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new `BBQueue` in a const context
    pub const fn new() -> Self {
        Self {
            sto: S::INIT,
            cor: C::INIT,
            not: N::INIT,
        }
    }
}

impl<S: Storage, C: Coord, N: Notifier> BBQueue<S, C, N> {
    /// Create a new [`FramedProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn framed_producer(&self) -> FramedProducer<&'_ Self> {
        FramedProducer {
            bbq: self,
            pd: PhantomData,
        }
    }

    /// Create a new [`FramedConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn framed_consumer(&self) -> FramedConsumer<&'_ Self> {
        FramedConsumer {
            bbq: self,
            pd: PhantomData,
        }
    }

    /// Create a new [`StreamProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn stream_producer(&self) -> StreamProducer<&'_ Self> {
        StreamProducer { bbq: self }
    }

    /// Create a new [`StreamConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub const fn stream_consumer(&self) -> StreamConsumer<&'_ Self> {
        StreamConsumer { bbq: self }
    }
}

#[cfg(feature = "alloc")]
impl<S: Storage, C: Coord, N: Notifier> crate::queue::ArcBBQueue<S, C, N> {
    /// Create a new [`FramedProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn framed_producer(&self) -> FramedProducer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        FramedProducer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    /// Create a new [`FramedConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn framed_consumer(&self) -> FramedConsumer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        FramedConsumer {
            bbq: self.0.bbq_ref(),
            pd: PhantomData,
        }
    }

    /// Create a new [`StreamProducer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn stream_producer(&self) -> StreamProducer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        StreamProducer {
            bbq: self.0.bbq_ref(),
        }
    }

    /// Create a new [`StreamConsumer`] for this [`BBQueue`]
    ///
    /// Although mixing stream and framed consumer/producers will not result in UB,
    /// it will also not work correctly.
    pub fn stream_consumer(&self) -> StreamConsumer<alloc::sync::Arc<BBQueue<S, C, N>>> {
        StreamConsumer {
            bbq: self.0.bbq_ref(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::traits::{
        coordination::cas::AtomicCoord, notifier::polling::Polling, storage::Inline,
    };

    use super::*;

    type Queue = BBQueue<Inline<4096>, AtomicCoord, Polling>;
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

    #[test]
    fn split_sanity_check() {
        let bb: BBQueue<Inline<10>, AtomicCoord, Polling> = BBQueue::new();
        let prod = bb.stream_producer();
        let cons = bb.stream_consumer();

        // Fill buffer
        let mut wgrant = prod.grant_exact(10).unwrap();
        assert_eq!(wgrant.len(), 10);
        wgrant.copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        wgrant.commit(10);

        let rgrant = cons.split_read().unwrap();
        assert_eq!(rgrant.combined_len(), 10);
        assert_eq!(
            rgrant.bufs(),
            (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10][..], &[][..])
        );
        // Release part of the buffer
        rgrant.release(6);

        // Almost fill buffer again => | 11 | 12 | 13 | 14 | 15 | x | 7 | 8 | 9 | 10 |
        let mut wgrant = prod.grant_exact(5).unwrap();
        assert_eq!(wgrant.len(), 5);
        wgrant.copy_from_slice(&[11, 12, 13, 14, 15]);
        wgrant.commit(5);

        let rgrant = cons.split_read().unwrap();
        assert_eq!(rgrant.combined_len(), 9);
        assert_eq!(
            rgrant.bufs(),
            (&[7, 8, 9, 10][..], &[11, 12, 13, 14, 15][..])
        );

        // Release part of the buffer => | x | x | x | 14 | 15 | x | x | x | x | x |
        rgrant.release(7);

        // Check that it is possible to claim exactly the remaining space
        assert!(prod.grant_exact(5).is_ok());

        // Check that it is not possible to claim more space than what should be available
        assert!(prod.grant_exact(6).is_err());

        // Fill buffer to the end => | x | x | x | 14 | 15 | 21 | 22 | 23 | 24 | 25 |
        let mut wgrant = prod.grant_exact(5).unwrap();
        wgrant.copy_from_slice(&[21, 22, 23, 24, 25]);
        wgrant.commit(5);

        let rgrant = cons.split_read().unwrap();
        assert_eq!(rgrant.combined_len(), 7);
        assert_eq!(rgrant.bufs(), (&[14, 15, 21, 22, 23, 24, 25][..], &[][..]));
        rgrant.release(0);

        // Fill buffer to the end => | 26 | 27 | x | 14 | 15 | 21 | 22 | 23 | 24 | 25 |
        let mut wgrant = prod.grant_exact(2).unwrap();
        wgrant.copy_from_slice(&[26, 27]);
        wgrant.commit(2);

        // Fill buffer to the end => | x | 27 | x | x | x | x | x | x | x | x |
        let rgrant = cons.split_read().unwrap();
        assert_eq!(rgrant.combined_len(), 9);
        assert_eq!(
            rgrant.bufs(),
            (&[14, 15, 21, 22, 23, 24, 25][..], &[26, 27][..])
        );
        rgrant.release(8);

        let rgrant = cons.split_read().unwrap();
        assert_eq!(rgrant.combined_len(), 1);
        assert_eq!(rgrant.bufs(), (&[27][..], &[][..]));
        rgrant.release(1);
    }
}
