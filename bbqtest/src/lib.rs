//! NOTE: this crate is really just a shim for testing
//! the other no-std crate.

mod multi_thread;
mod single_thread;

#[cfg(test)]
mod tests {
    use bbqueue::{bbq, BBQueue, Error as BBQError};

    #[test]
    fn deref_deref_mut() {
        let bb = bbq!(6).unwrap();

        let mut wgr = bb.grant(1).unwrap();

        // deref_mut
        wgr[0] = 123;

        assert_eq!(wgr.len(), 1);

        bb.commit(1, wgr);

        // deref
        let rgr = bb.read().unwrap();

        assert_eq!(rgr[0], 123);

        bb.release(1, rgr);
    }

    #[test]
    fn static_allocator() {
        // Check we can make multiple static items...
        let bbq1 = bbq!(8).unwrap();
        let bbq2 = bbq!(8).unwrap();

        // ... and they aren't the same
        let mut wgr1 = bbq1.grant(3).unwrap();
        wgr1.buf().copy_from_slice(&[1, 2, 3]);
        bbq1.commit(3, wgr1);

        // no data here...
        assert!(bbq2.read().is_err());

        // ...data is here!
        let rgr1 = bbq1.read().unwrap();
        assert_eq!(rgr1.buf(), &[1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn bad_commit() {
        // Check we can make multiple static items...
        let bbq1 = bbq!(8).unwrap();
        let bbq2 = bbq!(8).unwrap();

        // ... and they aren't the same
        let mut wgr1 = bbq1.grant(3).unwrap();
        wgr1.buf().copy_from_slice(&[1, 2, 3]);

        // then give the wrong one the grant
        bbq2.commit(3, wgr1);
    }

    #[test]
    #[should_panic]
    fn bad_release() {
        // Check we can make multiple static items...
        let bbq1 = bbq!(8).unwrap();
        let bbq2 = bbq!(8).unwrap();

        // ... and they aren't the same
        let mut wgr1 = bbq1.grant(3).unwrap();
        wgr1.buf().copy_from_slice(&[1, 2, 3]);
        bbq1.commit(3, wgr1);

        // Read from the first
        let rgr1 = bbq1.read().unwrap();
        assert_eq!(rgr1.buf(), &[1, 2, 3]);

        // Release from 2
        bbq2.release(1, rgr1);
    }

    #[test]
    fn create_queue() {
        // Create queue using "no_std" style
        static mut DATA: [u8; 6] = [0u8; 6];
        let mut _b = unsafe { BBQueue::unpinned_new(&mut DATA) };
        let (_prod, _cons) = _b.split();
    }

    #[test]
    fn create_boxed_queue() {
        // Create queue using leaky "boxed" style
        let bbq = BBQueue::new_boxed(1024);
        let (_prod, _cons) = BBQueue::split_box(bbq);
    }

    #[test]
    fn direct_usage_sanity() {
        // Initialize
        static mut DATA: [u8; 6] = [0u8; 6];
        let mut bb = unsafe { BBQueue::unpinned_new(&mut DATA) };
        assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

        // Initial grant, shouldn't roll over
        let mut x = bb.grant(4).unwrap();

        // Still no data available yet
        assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

        // Add full data from grant
        x.buf().copy_from_slice(&[1, 2, 3, 4]);

        // Still no data available yet
        assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

        // Commit data
        bb.commit(4, x);

        ::std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);

        let a = bb.read().unwrap();
        assert_eq!(a.buf(), &[1, 2, 3, 4]);

        // Release the first two bytes
        bb.release(2, a);

        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        bb.release(0, r);

        // Grant two more
        let mut x = bb.grant(2).unwrap();
        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        bb.release(0, r);

        // Add more data
        x.buf().copy_from_slice(&[11, 12]);
        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        bb.release(0, r);

        // Commit
        bb.commit(2, x);

        let a = bb.read().unwrap();
        assert_eq!(a.buf(), &[3, 4, 11, 12]);

        bb.release(2, a);
        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        bb.release(0, r);

        let mut x = bb.grant(3).unwrap();
        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        bb.release(0, r);

        x.buf().copy_from_slice(&[21, 22, 23]);

        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        bb.release(0, r);
        bb.commit(3, x);

        let a = bb.read().unwrap();

        // NOTE: The data we just added isn't available yet,
        // since it has wrapped around
        assert_eq!(a.buf(), &[11, 12]);

        bb.release(2, a);

        // And now we can see it
        let r = bb.read().unwrap();
        assert_eq!(r.buf(), &[21, 22, 23]);
        bb.release(0, r);

        // Ask for something way too big
        assert!(bb.grant(10).is_err());
    }

    #[test]
    fn spsc_usage_sanity() {
        let bb = bbq!(6).unwrap();

        let (mut tx, mut rx) = bb.split();
        assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

        // Initial grant, shouldn't roll over
        let mut x = tx.grant(4).unwrap();

        // Still no data available yet
        assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

        // Add full data from grant
        x.buf().copy_from_slice(&[1, 2, 3, 4]);

        // Still no data available yet
        assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

        // Commit data
        tx.commit(4, x);

        let a = rx.read().unwrap();
        assert_eq!(a.buf(), &[1, 2, 3, 4]);

        // Release the first two bytes
        rx.release(2, a);

        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        rx.release(0, r);

        // Grant two more
        let mut x = tx.grant(2).unwrap();
        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        rx.release(0, r);

        // Add more data
        x.buf().copy_from_slice(&[11, 12]);
        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[3, 4]);
        rx.release(0, r);

        // Commit
        tx.commit(2, x);

        let a = rx.read().unwrap();
        assert_eq!(a.buf(), &[3, 4, 11, 12]);
        rx.release(2, a);

        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        rx.release(0, r);

        let mut x = tx.grant(3).unwrap();
        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        rx.release(0, r);

        x.buf().copy_from_slice(&[21, 22, 23]);

        let r = rx.read().unwrap();
        assert_eq!(r.buf(), &[11, 12]);
        rx.release(0, r);
        tx.commit(3, x);

        let a = rx.read().unwrap();

        // NOTE: The data we just added isn't available yet,
        // since it has wrapped around
        assert_eq!(a.buf(), &[11, 12]);

        rx.release(2, a);

        // And now we can see it
        assert_eq!(rx.read().unwrap().buf(), &[21, 22, 23]);

        // Ask for something way too big
        assert!(tx.grant(10).is_err());
    }
}
