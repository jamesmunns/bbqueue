//! NOTE: this crate is really just a shim for testing
//! the other no-std crate.

// mod multi_thread;
// mod single_thread;

#[cfg(test)]
mod tests {
    use bbqueue::{
        BBQueue,
        Error as BBQError,
    };
    use generic_array::{
        typenum::*,
    };



    #[test]
    fn create_queue() {
        static mut DATA: [u8; 6] = [0u8; 6];
        let mut _b = BBQueue::new(unsafe { &mut DATA });
        let (prod, cons) = _b.split();
        // let (prod2, cons2) = _b.split();
    }

    // #[test]
    // fn direct_usage_sanity() {
    //     // Initialize
    //     let mut bb: BBQueue<U6> = BBQueue::new();
    //     assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

    //     // Initial grant, shouldn't roll over
    //     let x = bb.grant(4).unwrap();

    //     // Still no data available yet
    //     assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

    //     // Add full data from grant
    //     x.buf.copy_from_slice(&[1, 2, 3, 4]);

    //     // Still no data available yet
    //     assert_eq!(bb.read(), Err(BBQError::InsufficientSize));

    //     // Commit data
    //     bb.commit(4, x);

    //     let a = bb.read().unwrap();
    //     assert_eq!(a.buf, &[1, 2, 3, 4]);

    //     // Release the first two bytes
    //     bb.release(2, a);

    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     bb.release(0, r);

    //     // Grant two more
    //     let x = bb.grant(2).unwrap();
    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     bb.release(0, r);

    //     // Add more data
    //     x.buf.copy_from_slice(&[11, 12]);
    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     bb.release(0, r);

    //     // Commit
    //     bb.commit(2, x);

    //     let a = bb.read().unwrap();
    //     assert_eq!(a.buf, &[3, 4, 11, 12]);

    //     bb.release(2, a);
    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     bb.release(0, r);

    //     let x = bb.grant(3).unwrap();
    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     bb.release(0, r);

    //     x.buf.copy_from_slice(&[21, 22, 23]);

    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     bb.release(0, r);
    //     bb.commit(3, x);

    //     let a = bb.read().unwrap();

    //     // NOTE: The data we just added isn't available yet,
    //     // since it has wrapped around
    //     assert_eq!(a.buf, &[11, 12]);

    //     bb.release(2, a);

    //     // And now we can see it
    //     let r = bb.read().unwrap();
    //     assert_eq!(r.buf, &[21, 22, 23]);
    //     bb.release(0, r);

    //     // Ask for something way too big
    //     assert!(bb.grant(10).is_err());
    // }

    // #[test]
    // fn spsc_usage_sanity() {
    //     let mut bb: BBQueue<U6> = BBQueue::new();

    //     let (mut tx, mut rx) = bb.split();
    //     assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

    //     // Initial grant, shouldn't roll over
    //     let x = tx.grant(4).unwrap();

    //     // Still no data available yet
    //     assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

    //     // Add full data from grant
    //     x.buf.copy_from_slice(&[1, 2, 3, 4]);

    //     // Still no data available yet
    //     assert_eq!(rx.read(), Err(BBQError::InsufficientSize));

    //     // Commit data
    //     tx.commit(4, x);

    //     let a = rx.read().unwrap();
    //     assert_eq!(a.buf, &[1, 2, 3, 4]);

    //     // Release the first two bytes
    //     rx.release(2, a);

    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     rx.release(0, r);

    //     // Grant two more
    //     let x = tx.grant(2).unwrap();
    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     rx.release(0, r);

    //     // Add more data
    //     x.buf.copy_from_slice(&[11, 12]);
    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[3, 4]);
    //     rx.release(0, r);

    //     // Commit
    //     tx.commit(2, x);

    //     let a = rx.read().unwrap();
    //     assert_eq!(a.buf, &[3, 4, 11, 12]);
    //     rx.release(2, a);

    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     rx.release(0, r);

    //     let x = tx.grant(3).unwrap();
    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     rx.release(0, r);

    //     x.buf.copy_from_slice(&[21, 22, 23]);

    //     let r = rx.read().unwrap();
    //     assert_eq!(r.buf, &[11, 12]);
    //     rx.release(0, r);
    //     tx.commit(3, x);

    //     let a = rx.read().unwrap();

    //     // NOTE: The data we just added isn't available yet,
    //     // since it has wrapped around
    //     assert_eq!(a.buf, &[11, 12]);

    //     rx.release(2, a);

    //     // And now we can see it
    //     assert_eq!(rx.read().unwrap().buf, &[21, 22, 23]);

    //     // Ask for something way too big
    //     assert!(tx.grant(10).is_err());
    // }
}
