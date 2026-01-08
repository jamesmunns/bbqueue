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
//! ## Reading fixed-size chunks with `read_exact`
//!
//! When you need to read a specific amount of data (e.g., in an interrupt handler
//! or DMA operation), use `read_exact` instead of `read`. This allows you to hold
//! a smaller read grant, leaving more buffer space available for the producer to
//! continue writing.
//!
//! ```rust
//! use bbqueue::nicknames::Churrasco;
//!
//! // Create a buffer
//! let bb: Churrasco<1000> = Churrasco::new();
//! let prod = bb.stream_producer();
//! let cons = bb.stream_consumer();
//!
//! // Producer writes 200 bytes
//! let mut wgr = prod.grant_exact(200).unwrap();
//! for i in 0..200 {
//!     wgr[i] = i as u8;
//! }
//! wgr.commit(200);
//!
//! // Consumer reads exactly 100 bytes at a time
//! // This is useful in interrupt handlers where you process fixed-size chunks
//! let rgr = cons.read_exact(100).unwrap();
//! assert_eq!(rgr.len(), 100);
//! // Process the chunk...
//! rgr.release(100);
//!
//! // Read the remaining 100 bytes
//! let rgr = cons.read_exact(100).unwrap();
//! assert_eq!(rgr.len(), 100);
//! rgr.release(100);
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
    fn read_exact_basic() {
        use crate::traits::notifier::polling::Polling;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        // Write 20 bytes
        let write_data = &[
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14,
        ];
        let mut wgr = prod.grant_exact(20).unwrap();
        wgr.copy_from_slice(write_data);
        wgr.commit(20);

        // Read exactly 5 bytes
        let rgr = cons.read_exact(5).unwrap();
        assert_eq!(rgr.len(), 5);
        assert_eq!(rgr.deref(), &write_data[0..5]);
        rgr.release(5);

        // Read exactly 10 bytes
        let rgr = cons.read_exact(10).unwrap();
        assert_eq!(rgr.len(), 10);
        assert_eq!(rgr.deref(), &write_data[5..15]);
        rgr.release(10);

        // Read exactly 5 bytes
        let rgr = cons.read_exact(5).unwrap();
        assert_eq!(rgr.len(), 5);
        assert_eq!(rgr.deref(), &write_data[15..20]);
        rgr.release(5);

        // Buffer should be empty
        assert!(cons.read().is_err());
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn read_exact_insufficient() {
        use crate::traits::{coordination::ReadGrantError, notifier::polling::Polling};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        // Write 10 bytes
        let mut wgr = prod.grant_exact(10).unwrap();
        wgr[..10].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        wgr.commit(10);

        // Try to read 15 bytes (more than available)
        let result = cons.read_exact(15);
        assert!(matches!(result, Err(ReadGrantError::InsufficientSize)));

        // Original data should still be available
        let rgr = cons.read().unwrap();
        assert_eq!(rgr.len(), 10);
        rgr.release(10);
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn read_exact_partial_release() {
        use crate::traits::notifier::polling::Polling;

        static BBQ: BBQueue<Inline<200>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        // Write 100 bytes
        let mut wgr = prod.grant_exact(100).unwrap();
        for i in 0..100 {
            wgr[i] = i as u8;
        }
        wgr.commit(100);

        // Read exactly 10 bytes at a time and release them
        for chunk in 0..10 {
            let rgr = cons.read_exact(10).unwrap();
            assert_eq!(rgr.len(), 10);
            for i in 0..10 {
                assert_eq!(rgr[i], (chunk * 10 + i) as u8);
            }
            rgr.release(10);
        }

        // Buffer should be empty
        assert!(cons.read().is_err());
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn read_exact_with_concurrent_writes() {
        use crate::traits::notifier::polling::Polling;

        static BBQ: BBQueue<Inline<1000>, AtomicCoord, Polling> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        // Write initial 100 bytes
        let mut wgr = prod.grant_exact(100).unwrap();
        for i in 0..100 {
            wgr[i] = i as u8;
        }
        wgr.commit(100);

        // Read exactly 50 bytes (leaving 50 in buffer)
        let rgr = cons.read_exact(50).unwrap();
        assert_eq!(rgr.len(), 50);

        // While holding the read grant, producer should be able to write more
        // This simulates the use case from the issue where the producer continues
        // writing while consumer processes a small chunk
        let mut wgr2 = prod.grant_exact(100).unwrap();
        for i in 0..100 {
            wgr2[i] = (100 + i) as u8;
        }
        wgr2.commit(100);

        // Release the first read
        rgr.release(50);

        // Now read the next 50 bytes (from first write)
        let rgr = cons.read_exact(50).unwrap();
        assert_eq!(rgr.len(), 50);
        for i in 0..50 {
            assert_eq!(rgr[i], (50 + i) as u8);
        }
        rgr.release(50);

        // And read the second write
        let rgr = cons.read_exact(100).unwrap();
        assert_eq!(rgr.len(), 100);
        for i in 0..100 {
            assert_eq!(rgr[i], (100 + i) as u8);
        }
        rgr.release(100);
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
    async fn asink_read_exact() {
        static BBQ: BBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> = BBQueue::new();
        let prod = BBQ.stream_producer();
        let cons = BBQ.stream_consumer();

        let rxfut = tokio::task::spawn(async move {
            // Wait for exactly 10 bytes
            let rgr = cons.wait_read_exact(10).await;
            assert_eq!(rgr.len(), 10);
            assert_eq!(rgr.deref(), &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        });

        let txfut = tokio::task::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut wgr = prod.grant_exact(10).unwrap();
            wgr.copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            wgr.commit(10);
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
