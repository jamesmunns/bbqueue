//! bbq2
//!
//! A new and improved bipbuffer queue.

#![cfg_attr(not(any(test, feature = "std")), no_std)]

/// Type aliases for different generic configurations
///
pub mod nicknames;

/// Producer and consumer interfaces
///
pub mod prod_cons;

/// Queue storage
///
pub mod queue;

/// Generic traits
///
pub mod traits;

/// Re-export of external types/traits
///
pub mod export {
    pub use const_init::ConstInit;
}

#[cfg(all(test, feature = "std"))]
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

    #[cfg(all(target_has_atomic = "ptr", feature = "std"))]
    #[test]
    fn ux() {
        use crate::traits::{notifier::blocking::Blocking, storage::BoxedSlice};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let _ = BBQ.stream_producer();
        let _ = BBQ.stream_consumer();

        let buf2 = Inline::<64>::new();
        let bbq2: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(&buf2);
        let _ = bbq2.stream_producer();
        let _ = bbq2.stream_consumer();

        let buf3 = BoxedSlice::new(64);
        let bbq3: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.stream_producer();
        let _ = bbq3.stream_consumer();
    }

    #[cfg(target_has_atomic = "ptr")]
    #[test]
    fn smoke() {
        use crate::traits::notifier::blocking::Blocking;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
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
        use crate::traits::notifier::blocking::Blocking;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
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
        use crate::traits::notifier::blocking::Blocking;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
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
