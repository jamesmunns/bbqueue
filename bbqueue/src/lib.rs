//! # BBQueue
//!
//! BBQueue, short for "BipBuffer Queue", is a Single Producer Single Consumer,
//! lockless, no_std, thread safe, queue, based on [BipBuffers]. For more info on
//! the design of the lock-free algorithm used by bbqueue, see [this blog post].
//!
//! [BipBuffers]: https://www.codeproject.com/articles/The-Bip-Buffer-The-Circular-Buffer-with-a-Twist
//! [this blog post]: https://ferrous-systems.com/blog/lock-free-ring-buffer/
//!
//! BBQueue is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
//! systems.
//!
//! While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
//! main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
//! block of contiguous memory, which can be filled (or emptied) by a DMA engine.
//!
//! ## Local usage
//!
//! ```rust
//! // The "Churrasco" flavor has inline storage, hardware atomic
//! // support, no async support, and is not reference counted.
//! use bbqueue::nicknames::Churrasco;
//!
//! // Create a buffer with six elements
//! let bb: Churrasco<6> = Churrasco::new();
//! let prod = bb.stream_producer();
//! let cons = bb.stream_consumer();
//!
//! // Request space for one byte
//! let mut wgr = prod.grant_exact(1).unwrap();
//!
//! // Set the data
//! wgr[0] = 123;
//!
//! assert_eq!(wgr.len(), 1);
//!
//! // Make the data ready for consuming
//! wgr.commit(1);
//!
//! // Read all available bytes
//! let rgr = cons.read().unwrap();
//!
//! assert_eq!(rgr[0], 123);
//!
//! // Release the space for later writes
//! rgr.release(1);
//! ```
//!
//! ## Static usage
//!
//! ```rust
//! use bbqueue::nicknames::Churrasco;
//! use std::{thread::{sleep, spawn}, time::Duration};
//!
//! // Create a buffer with six elements
//! static BB: Churrasco<6> = Churrasco::new();
//!
//! fn receiver() {
//!     let cons = BB.stream_consumer();
//!     loop {
//!         if let Ok(rgr) = cons.read() {
//!             assert_eq!(rgr.len(), 1);
//!             assert_eq!(rgr[0], 123);
//!             rgr.release(1);
//!             break;
//!         }
//!         // don't do this in real code, use Notify!
//!         sleep(Duration::from_millis(10));
//!     }
//! }
//!
//! fn main() {
//!     let prod = BB.stream_producer();
//!
//!     // spawn the consumer
//!     let hdl = spawn(receiver);
//!
//!     // Request space for one byte
//!     let mut wgr = prod.grant_exact(1).unwrap();
//!
//!     // Set the data
//!     wgr[0] = 123;
//!
//!     assert_eq!(wgr.len(), 1);
//!
//!     // Make the data ready for consuming
//!     wgr.commit(1);
//!
//!     // make sure the receiver terminated
//!     hdl.join().unwrap();
//! }
//! ```
//!
//! ## Nicknames
//!
//! bbqueue uses generics to customize the data structure in four main ways:
//!
//! * Whether the byte storage is inline (and const-generic), or heap allocated
//! * Whether the queue is polling-only, or supports async/await sending/receiving
//! * Whether the queue uses a lock-free algorithm with CAS atomics, or uses a critical section
//!   (for targets that don't have CAS atomics)
//! * Whether the queue is reference counted, allowing Producer and Consumer halves to be passed
//!   around without lifetimes.
//!
//! See the [`nicknames`](crate::nicknames) module for all sixteen variants.
//!
//! ## Stability
//!
//! `bbqueue` v0.6 is a breaking change from the older "classic" v0.5 interfaces. The intent is to
//! have a few minor breaking changes in early 2026, and to get to v1.0 as quickly as possible.

#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "alloc")]
extern crate alloc;

/// Type aliases for different generic configurations
///
pub mod nicknames;

/// Producer and consumer interfaces
///
pub mod prod_cons;

/// Queue storage
///
mod queue;
#[cfg(feature = "alloc")]
pub use queue::ArcBBQueue;
pub use queue::BBQueue;

/// Generic traits
///
pub mod traits;

/// Re-export of external types/traits
///
pub mod export {
    pub use const_init::ConstInit;
}

#[cfg(all(test, feature = "alloc"))]
mod test {
    use core::{ops::Deref, time::Duration};

    use crate::{
        queue::{ArcBBQueue, BBQueue},
        traits::{
            coordination::cas::AtomicCoord,
            notifier::maitake::MaiNotSpsc,
            storage::{BoxedSlice, Inline},
        },
    };

    #[cfg(all(target_has_atomic = "ptr", feature = "alloc"))]
    #[test]
    fn ux() {
        use crate::traits::{notifier::polling::Polling, storage::BoxedSlice};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let _ = BBQ.stream_producer();
        let _ = BBQ.stream_consumer();

        let buf2 = Inline::<64>::new();
        let bbq2: BBQueue<_, AtomicCoord, Polling> = BBQueue::new_with_storage(&buf2);
        let _ = bbq2.stream_producer();
        let _ = bbq2.stream_consumer();

        let buf3 = BoxedSlice::new(64);
        let bbq3: BBQueue<_, AtomicCoord, Polling> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.stream_producer();
        let _ = bbq3.stream_consumer();
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn smoke() {
        use crate::traits::notifier::polling::Polling;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        let write_once = &[0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14];
        let mut wgr = prod.grant_exact(8).unwrap();
        wgr.copy_from_slice(write_once);
        wgr.commit(8);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), write_once.as_slice(),);
        rgr.release(4);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), &write_once[4..]);
        rgr.release(4);

        assert!(cons.read().is_err());
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn smoke_framed() {
        use crate::traits::notifier::polling::Polling;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.framed_producer();
        let cons = BBQ.framed_consumer();

        let write_once = &[0x01, 0x02, 0x03, 0x04, 0x11, 0x12];
        let mut wgr = prod.grant(8).unwrap();
        wgr[..6].copy_from_slice(write_once);
        wgr.commit(6);

        let rgr = cons.read().unwrap();
        assert_eq!(rgr.deref(), write_once.as_slice());
        rgr.release();

        assert!(cons.read().is_err());
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn framed_misuse() {
        use crate::traits::notifier::polling::Polling;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.framed_consumer();

        // Bad grant one: HUGE header value
        let write_once = &[0xFF, 0xFF, 0x03, 0x04, 0x11, 0x12];
        let mut wgr = prod.grant_exact(6).unwrap();
        wgr[..6].copy_from_slice(write_once);
        wgr.commit(6);

        assert!(cons.read().is_err());

        {
            // Clear the bad grant
            let cons2 = BBQ.stream_consumer();
            let rgr = cons2.read().unwrap();
            rgr.release(6);
        }

        // Bad grant two: too small of a grant
        let write_once = &[0x00];
        let mut wgr = prod.grant_exact(1).unwrap();
        wgr[..1].copy_from_slice(write_once);
        wgr.commit(1);

        assert!(cons.read().is_err());
    }

    #[tokio::test]
    async fn asink() {
        static BBQ: BBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        let rxfut = tokio::task::spawn(async move {
            let rgr = cons.wait_read().await;
            assert_eq!(rgr.deref(), &[1, 2, 3]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant_exact(3).unwrap();
            wgr.copy_from_slice(&[1, 2, 3]);
            wgr.commit(3);
        });

        // todo: timeouts
        rxfut.await.unwrap();
        txfut.await.unwrap();
    }

    #[tokio::test]
    async fn asink_framed() {
        static BBQ: BBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> = BBQueue::new();
        let prod = BBQ.framed_producer();
        let cons = BBQ.framed_consumer();

        let rxfut = tokio::task::spawn(async move {
            let rgr = cons.wait_read().await;
            assert_eq!(rgr.deref(), &[1, 2, 3]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant(3).unwrap();
            wgr.copy_from_slice(&[1, 2, 3]);
            wgr.commit(3);
        });

        // todo: timeouts
        rxfut.await.unwrap();
        txfut.await.unwrap();
    }

    #[tokio::test]
    async fn arc1() {
        let bbq: ArcBBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> =
            ArcBBQueue::new_with_storage(Inline::new());
        let prod = bbq.stream_producer();
        let cons = bbq.stream_consumer();

        let rxfut = tokio::task::spawn(async move {
            let rgr = cons.wait_read().await;
            assert_eq!(rgr.deref(), &[1, 2, 3]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant_exact(3).unwrap();
            wgr.copy_from_slice(&[1, 2, 3]);
            wgr.commit(3);
        });

        // todo: timeouts
        rxfut.await.unwrap();
        txfut.await.unwrap();
    }

    #[tokio::test]
    async fn arc2() {
        let bbq: ArcBBQueue<BoxedSlice, AtomicCoord, MaiNotSpsc> =
            ArcBBQueue::new_with_storage(BoxedSlice::new(64));
        let prod = bbq.stream_producer();
        let cons = bbq.stream_consumer();

        let rxfut = tokio::task::spawn(async move {
            let rgr = cons.wait_read().await;
            assert_eq!(rgr.deref(), &[1, 2, 3]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant_exact(3).unwrap();
            wgr.copy_from_slice(&[1, 2, 3]);
            wgr.commit(3);
        });

        // todo: timeouts
        rxfut.await.unwrap();
        txfut.await.unwrap();

        drop(bbq);
    }
}

/// Aliasing, panic-safety, and cross-thread behaviour of the framed API.
///
/// These earn their keep under miri rather than as plain unit tests: each one
/// exercises a pattern where a stale pointer, a double release, or a torn
/// coordinator update would be silent under a normal run.
#[cfg(all(test, feature = "std"))]
mod soundness_test {
    use std::sync::Arc;

    use const_init::ConstInit;

    use crate::{
        queue::BBQueue,
        traits::{
            bbqhdl::BbqHandle,
            coordination::cas::AtomicCoord,
            notifier::{Notifier, polling::Polling},
            storage::BoxedSlice,
        },
    };

    type Ring = BBQueue<BoxedSlice, AtomicCoord, Polling>;

    fn ring() -> Arc<Ring> {
        Arc::new(BBQueue::new_with_storage(BoxedSlice::new(64)))
    }

    /// Wrap the ring repeatedly at unaligned offsets, touching every byte of
    /// every grant so miri sees each access.
    #[test]
    fn wraparound_with_a_usize_header() {
        let bbq = ring();
        let prod = bbq.framed_producer::<usize>();
        let cons = bbq.framed_consumer::<usize>();

        // The header is 8 bytes, so bodies of 1..=9 make grants of 9..=17 and
        // the ring inverts at a different offset each time around.
        for round in 0..64usize {
            let body = (round % 9) + 1;
            let mut wgr = prod.grant(body).expect("space available");
            assert_eq!(wgr.len(), body);
            for (i, b) in wgr.iter_mut().enumerate() {
                *b = (round + i) as u8;
            }
            wgr.commit(body);

            let rgr = cons.read().expect("frame available");
            assert_eq!(rgr.len(), body, "round {round}");
            for (i, b) in rgr.iter().enumerate() {
                assert_eq!(*b, (round + i) as u8, "round {round} byte {i}");
            }
            rgr.release();
        }
    }

    /// A write grant taken and written through while a read grant into the same
    /// allocation is still live. The read grant must not observe the write.
    #[test]
    fn a_write_grant_does_not_disturb_a_live_read_grant() {
        let bbq = ring();
        let prod = bbq.framed_producer::<usize>();
        let cons = bbq.framed_consumer::<usize>();

        let mut seed = prod.grant(4).expect("space");
        seed.copy_from_slice(&[1, 2, 3, 4]);
        seed.commit(4);

        let rgr = cons.read().expect("frame");
        assert_eq!(&*rgr, &[1, 2, 3, 4]);

        let mut wgr = prod.grant(4).expect("space");
        wgr.copy_from_slice(&[9, 9, 9, 9]);
        assert_eq!(&*rgr, &[1, 2, 3, 4], "read grant must be unperturbed");
        wgr.commit(4);
        assert_eq!(&*rgr, &[1, 2, 3, 4]);
        rgr.release();

        let rgr2 = cons.read().expect("frame");
        assert_eq!(&*rgr2, &[9, 9, 9, 9]);
        rgr2.release();
    }

    /// The producer and consumer halves on separate threads, wrapping the ring.
    #[test]
    fn producer_and_consumer_wrap_the_ring_from_two_threads() {
        let bbq = ring();
        let prod = bbq.framed_producer::<usize>();
        let cons = bbq.framed_consumer::<usize>();

        const N: usize = 40;

        let tx = std::thread::spawn(move || {
            for round in 0..N {
                let body = (round % 7) + 1;
                loop {
                    match prod.grant(body) {
                        Ok(mut wgr) => {
                            for b in wgr.iter_mut() {
                                *b = round as u8;
                            }
                            wgr.commit(body);
                            break;
                        }
                        Err(_) => std::thread::yield_now(),
                    }
                }
            }
        });

        let rx = std::thread::spawn(move || {
            for round in 0..N {
                loop {
                    match cons.read() {
                        Ok(rgr) => {
                            assert_eq!(rgr.len(), (round % 7) + 1);
                            for b in rgr.iter() {
                                assert_eq!(*b, round as u8);
                            }
                            rgr.release();
                            break;
                        }
                        Err(_) => std::thread::yield_now(),
                    }
                }
            }
        });

        tx.join().expect("producer");
        rx.join().expect("consumer");
    }

    /// Wakes by panicking, so a grant that unwinds mid-commit takes the queue
    /// handle down with it.
    struct PanicNotifier;

    impl ConstInit for PanicNotifier {
        const INIT: Self = PanicNotifier;
    }

    impl Notifier for PanicNotifier {
        fn wake_one_consumer(&self) {
            panic!("wake_one_consumer");
        }
        fn wake_one_producer(&self) {
            panic!("wake_one_producer");
        }
    }

    type PanicRing = BBQueue<BoxedSlice, AtomicCoord, PanicNotifier>;

    fn panic_ring() -> Arc<PanicRing> {
        Arc::new(BBQueue::new_with_storage(BoxedSlice::new(64)))
    }

    fn swallow_panic(f: impl FnOnce()) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
    }

    #[test]
    fn a_commit_that_unwinds_releases_its_handle_exactly_once() {
        let bbq = panic_ring();
        let weak = Arc::downgrade(&bbq);
        let prod = bbq.framed_producer::<usize>();
        let baseline = Arc::strong_count(&bbq);

        let wgr = prod.grant(4).expect("space");
        assert_eq!(Arc::strong_count(&bbq), baseline + 1);

        assert!(swallow_panic(|| wgr.commit(4)), "the wake should panic");
        assert_eq!(Arc::strong_count(&bbq), baseline);

        drop(prod);
        drop(bbq);
        assert!(weak.upgrade().is_none(), "no leaked strong handle");
    }

    #[test]
    fn a_release_that_unwinds_releases_its_handle_exactly_once() {
        let bbq = panic_ring();
        let weak = Arc::downgrade(&bbq);
        let prod = bbq.framed_producer::<usize>();
        let cons = bbq.framed_consumer::<usize>();

        let wgr = prod.grant(4).expect("space");
        swallow_panic(|| wgr.commit(4));

        let baseline = Arc::strong_count(&bbq);
        let rgr = cons.read().expect("frame");
        assert_eq!(Arc::strong_count(&bbq), baseline + 1);

        assert!(swallow_panic(|| rgr.release()), "the wake should panic");
        assert_eq!(Arc::strong_count(&bbq), baseline);

        drop(prod);
        drop(cons);
        drop(bbq);
        assert!(weak.upgrade().is_none(), "no leaked strong handle");
    }

    /// The frame is published and the write slot reopened even though the wake
    /// unwound partway through the commit.
    #[test]
    fn a_commit_that_unwinds_still_publishes_its_frame() {
        let bbq = panic_ring();
        let prod = bbq.framed_producer::<usize>();
        let cons = bbq.framed_consumer::<usize>();

        let mut wgr = prod.grant(4).expect("space");
        wgr.copy_from_slice(&[7, 7, 7, 7]);
        swallow_panic(|| wgr.commit(4));

        let rgr = cons.read().expect("frame is published despite the panic");
        assert_eq!(&*rgr, &[7, 7, 7, 7]);
        swallow_panic(|| rgr.release());

        prod.grant(4)
            .expect("the in-progress write must have been cleared")
            .abort();
    }
}
