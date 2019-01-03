//! NOTE: this crate is really just a shim for testing
//! the other no-std crate.

mod multi_thread;
mod single_thread;

#[cfg(test)]
mod tests {
    use bbqueue::BBQueue;
    use generic_array::{
        typenum::*,
    };

    #[test]
    fn create_queue() {
        let _b: BBQueue<U6> = BBQueue::new();
    }

    #[test]
    fn direct_usage_sanity() {
        // Initialize
        let mut bb: BBQueue<U6> = BBQueue::new();
        assert_eq!(bb.read().buf, &[]);

        // Initial grant, shouldn't roll over
        let x = bb.grant(4).unwrap();

        // Still no data available yet
        assert_eq!(bb.read().buf, &[]);

        // Add full data from grant
        x.buf.copy_from_slice(&[1, 2, 3, 4]);

        // Still no data available yet
        assert_eq!(bb.read().buf, &[]);

        // Commit data
        bb.commit(4, x);

        let a = bb.read();
        assert_eq!(a.buf, &[1, 2, 3, 4]);

        // Release the first two bytes
        bb.release(2, a);

        assert_eq!(bb.read().buf, &[3, 4]);

        // Grant two more
        let x = bb.grant(2).unwrap();
        assert_eq!(bb.read().buf, &[3, 4]);

        // Add more data
        x.buf.copy_from_slice(&[11, 12]);
        assert_eq!(bb.read().buf, &[3, 4]);

        // Commit
        bb.commit(2, x);

        let a = bb.read();
        assert_eq!(bb.read().buf, &[3, 4, 11, 12]);

        bb.release(2, a);
        assert_eq!(bb.read().buf, &[11, 12]);

        let x = bb.grant(3).unwrap();
        assert_eq!(bb.read().buf, &[11, 12]);

        x.buf.copy_from_slice(&[21, 22, 23]);

        assert_eq!(bb.read().buf, &[11, 12]);
        bb.commit(3, x);

        let a = bb.read();

        // NOTE: The data we just added isn't available yet,
        // since it has wrapped around
        assert_eq!(a.buf, &[11, 12]);

        bb.release(2, a);

        // And now we can see it
        assert_eq!(bb.read().buf, &[21, 22, 23]);

        // Ask for something way too big
        assert!(bb.grant(10).is_err());
    }

    #[test]
    fn spsc_usage_sanity() {
        let mut bb: BBQueue<U6> = BBQueue::new();

        let (mut tx, mut rx) = bb.split();
        assert_eq!(rx.read().buf, &[]);

        // Initial grant, shouldn't roll over
        let x = tx.grant(4).unwrap();

        // Still no data available yet
        assert_eq!(rx.read().buf, &[]);

        // Add full data from grant
        x.buf.copy_from_slice(&[1, 2, 3, 4]);

        // Still no data available yet
        assert_eq!(rx.read().buf, &[]);

        // Commit data
        tx.commit(4, x);

        let a = rx.read();
        assert_eq!(a.buf, &[1, 2, 3, 4]);

        // Release the first two bytes
        rx.release(2, a);

        assert_eq!(rx.read().buf, &[3, 4]);

        // Grant two more
        let x = tx.grant(2).unwrap();
        assert_eq!(rx.read().buf, &[3, 4]);

        // Add more data
        x.buf.copy_from_slice(&[11, 12]);
        assert_eq!(rx.read().buf, &[3, 4]);

        // Commit
        tx.commit(2, x);

        let a = rx.read();
        assert_eq!(rx.read().buf, &[3, 4, 11, 12]);

        rx.release(2, a);
        assert_eq!(rx.read().buf, &[11, 12]);

        let x = tx.grant(3).unwrap();
        assert_eq!(rx.read().buf, &[11, 12]);

        x.buf.copy_from_slice(&[21, 22, 23]);

        assert_eq!(rx.read().buf, &[11, 12]);
        tx.commit(3, x);

        let a = rx.read();

        // NOTE: The data we just added isn't available yet,
        // since it has wrapped around
        assert_eq!(a.buf, &[11, 12]);

        rx.release(2, a);

        // And now we can see it
        assert_eq!(rx.read().buf, &[21, 22, 23]);

        // Ask for something way too big
        assert!(tx.grant(10).is_err());
    }
}
