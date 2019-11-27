//! NOTE: this crate is really just a shim for testing
//! the other no-std crate.

// mod multi_thread;
mod single_thread;

#[cfg(test)]
mod tests {
    use bbqueue::{consts::*, BBBuffer, ConstBBBuffer, Error as BBQError, GrantR, GrantW, consts::*, ArrayLength};

    #[test]
    fn deref_deref_mut() {
        let bb: BBBuffer<U6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        let mut wgr = prod.grant_exact(1).unwrap();

        // deref_mut
        wgr[0] = 123;

        assert_eq!(wgr.len(), 1);

        wgr.commit(1);

        // deref
        let rgr = cons.read().unwrap();

        assert_eq!(rgr[0], 123);

        rgr.release(1);
    }

    #[test]
    fn static_allocator() {
        // Check we can make multiple static items...
        static BBQ1: BBBuffer<U6> = BBBuffer(ConstBBBuffer::new());
        static BBQ2: BBBuffer<U6> = BBBuffer(ConstBBBuffer::new());
        let (mut prod1, mut cons1) = BBQ1.try_split().unwrap();
        let (mut _prod2, mut cons2) = BBQ2.try_split().unwrap();

        // ... and they aren't the same
        let mut wgr1 = prod1.grant_exact(3).unwrap();
        wgr1.copy_from_slice(&[1, 2, 3]);
        wgr1.commit(3);

        // no data here...
        assert!(cons2.read().is_err());

        // ...data is here!
        let rgr1 = cons1.read().unwrap();
        assert_eq!(&*rgr1, &[1, 2, 3]);
    }

    fn debug_grant_w<'a, N: ArrayLength<u8>>(data: &mut GrantW<'a, N>) {
        let x = data.buf();
        println!("\n=====--=====");
        println!("Write:");
        dbg!(x.as_ptr());
        dbg!(x.len());
        println!("=====--=====\n");
    }

    fn debug_grant_r<'a, N: ArrayLength<u8>>(data: &GrantR<'a, N>) {
        let x = data.buf();
        println!("\n=====--=====");
        println!("Read:");
        dbg!(x.as_ptr());
        dbg!(x.len());
        println!("=====--=====\n");
    }

    #[test]
    fn direct_usage_sanity() {
        // Initialize
        let bb: BBBuffer<U6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();
        assert_eq!(cons.read(), Err(BBQError::InsufficientSize));

        // Initial grant, shouldn't roll over
        let mut x = prod.grant_exact(4).unwrap();
        // debug_grant_w(&mut x);

        // Still no data available yet
        assert_eq!(cons.read(), Err(BBQError::InsufficientSize));

        // Add full data from grant
        x.copy_from_slice(&[1, 2, 3, 4]);

        // Still no data available yet
        assert_eq!(cons.read(), Err(BBQError::InsufficientSize));

        // Commit data
        x.commit(4);

        ::std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);

        let a = cons.read().unwrap();
        // debug_grant_r(&a);
        assert_eq!(&*a, &[1, 2, 3, 4]);

        // Release the first two bytes
        a.release(2);

        let r = cons.read().unwrap();
        // debug_grant_r(&r);
        assert_eq!(&*r, &[3, 4]);
        r.release(0);

        // Grant two more
        let mut x = prod.grant_exact(2).unwrap();
        // debug_grant_w(&mut x);
        let r = cons.read().unwrap();
        // debug_grant_r(&r);
        assert_eq!(&*r, &[3, 4]);
        r.release(0);

        // Add more data
        x.copy_from_slice(&[11, 12]);
        // let r = cons.read().unwrap();
        // assert_eq!(&*r, &[3, 4]);
        // r.release(0);

    //     // Commit
    //     x.commit(2);

    //     let a = cons.read().unwrap();
    //     assert_eq!(&*a, &[3, 4, 11, 12]);

    //     a.release(2);
    //     let r = cons.read().unwrap();
    //     assert_eq!(&*r, &[11, 12]);
    //     r.release(0);

    //     let mut x = prod.grant_exact(3).unwrap();
    //     let r = cons.read().unwrap();
    //     assert_eq!(&*r, &[11, 12]);
    //     r.release(0);

    //     x.copy_from_slice(&[21, 22, 23]);

    //     let r = cons.read().unwrap();
    //     assert_eq!(&*r, &[11, 12]);
    //     r.release(0);
    //     x.commit(3);

    //     let a = cons.read().unwrap();

    //     // NOTE: The data we just added isn't available yet,
    //     // since it has wrapped around
    //     assert_eq!(&*a, &[11, 12]);

    //     a.release(2);

    //     // And now we can see it
    //     let r = cons.read().unwrap();
    //     assert_eq!(&*r, &[21, 22, 23]);
    //     r.release(0);

    //     // Ask for something way too big
    //     assert!(prod.grant_exact(10).is_err());
    }

    #[test]
    fn zero_sized_grant() {
        let bb: BBBuffer<U1000> = BBBuffer::new();
        let (mut prod, mut _cons) = bb.try_split().unwrap();

        let size = 1000;
        let grant = prod.grant_exact(size).unwrap();
        grant.commit(size);

        let grant = prod.grant_exact(0).unwrap();
        grant.commit(0);
    }
}
