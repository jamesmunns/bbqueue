#![allow(clippy::result_unit_err)]
#![cfg_attr(not(any(test, feature = "std")), no_std)]

pub mod nicknames;
pub mod prod_cons;
pub mod queue;
pub mod traits;

#[cfg(test)]
mod test {
    use core::{ops::Deref, time::Duration};

    use crate::{
        queue::{ArcBBQueue, BBQueue},
        traits::{
            coordination::cas::AtomicCoord,
            notifier::MaiNotSpsc,
            storage::{BoxedSlice, Inline},
        },
    };

    #[cfg(all(feature = "cas-atomics", feature = "std"))]
    #[test]
    fn ux() {
        use crate::traits::{notifier::Blocking, storage::BoxedSlice};

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let _ = BBQ.split_borrowed().unwrap();

        let buf2 = Inline::<64>::new();
        let bbq2: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(&buf2);
        let _ = bbq2.split_borrowed().unwrap();

        let buf3 = BoxedSlice::new(64);
        let bbq3: BBQueue<_, AtomicCoord, Blocking> = BBQueue::new_with_storage(buf3);
        let _ = bbq3.split_borrowed().unwrap();

        assert!(BBQ.split_borrowed().is_err());
        assert!(bbq2.split_borrowed().is_err());
        assert!(bbq3.split_borrowed().is_err());
    }

    #[cfg(feature = "cas-atomics")]
    #[test]
    fn smoke() {
        use crate::traits::notifier::Blocking;
        use core::ops::Deref;

        static BBQ: BBQueue<Inline<64>, AtomicCoord, Blocking> = BBQueue::new();
        let (prod, cons) = BBQ.split_borrowed().unwrap();

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

    #[tokio::test]
    async fn asink() {
        static BBQ: BBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> = BBQueue::new();
        let (prod, cons) = BBQ.split_borrowed().unwrap();

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
    async fn arc1() {
        let bbq: ArcBBQueue<Inline<64>, AtomicCoord, MaiNotSpsc> =
            ArcBBQueue::new_with_storage(Inline::new());
        let (prod, cons) = bbq.split_arc().unwrap();

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
        let (prod, cons) = bbq.split_arc().unwrap();

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
}
