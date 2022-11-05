#[cfg(test)]
mod tests {
    use bbqueue::BBBuffer;
    use bbqueue::Error;
    use futures::{executor::block_on, future::join};

    #[test]
    fn test_read() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        {
            let mut grant = prod.grant_exact(4).unwrap();
            let buf = grant.buf();
            buf[0] = 0xDE;
            buf[1] = 0xAD;
            buf[2] = 0xC0;
            buf[3] = 0xDE;
            grant.commit(4);
        }

        let r_grant = block_on(cons.read_async()).unwrap();

        assert_eq!(4, r_grant.len());
        assert_eq!(r_grant[0], 0xDE);
        assert_eq!(r_grant[1], 0xAD);
        assert_eq!(r_grant[2], 0xC0);
        assert_eq!(r_grant[3], 0xDE);
    }

    #[test]
    fn test_write() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        let mut w_grant = block_on(prod.grant_exact_async(4)).unwrap();
        assert_eq!(4, w_grant.len());
        w_grant[0] = 0xDE;
        w_grant[1] = 0xAD;
        w_grant[2] = 0xC0;
        w_grant[3] = 0xDE;
        w_grant.commit(4);

        let grant = cons.read().unwrap();
        let rx_buf = grant.buf();
        assert_eq!(4, rx_buf.len());
        assert_eq!(rx_buf[0], 0xDE);
        assert_eq!(rx_buf[1], 0xAD);
        assert_eq!(rx_buf[2], 0xC0);
        assert_eq!(rx_buf[3], 0xDE);
    }

    #[test]
    fn test_read_after_write() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();

        let read_fut = async {
            let r_grant = cons.read_async().await.unwrap();
            r_grant.release(4);

            let time = std::time::Instant::now(); // TODO: Remove time dependence in test
            #[cfg(feature = "verbose")]
            println!("Read completed at {:?}", time);
            time
        };

        let write_fut = async {
            let mut w_grant = prod.grant_exact_async(4).await.unwrap();
            w_grant[0] = 0xDE;
            w_grant[1] = 0xAD;
            w_grant[2] = 0xC0;
            w_grant[3] = 0xDE;
            w_grant.commit(4);

            let time = std::time::Instant::now(); // TODO: Remove time dependence in test
            #[cfg(feature = "verbose")]
            println!("Write completed at {:?}", time);
            time
        };

        let (r_time, w_time) = block_on(join(read_fut, write_fut));
        assert!(r_time > w_time)
    }

    #[test]
    fn grant_exact_too_big() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut _cons) = bb.try_split().unwrap();
        let w_grant_res = block_on(async { prod.grant_exact_async(8).await });

        assert_eq!(w_grant_res.unwrap_err(), Error::InsufficientSize);
    }

    #[test]
    fn grant_exact_loop() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();
        let w_grant = prod.grant_exact(4).unwrap();
        w_grant.commit(4);

        let read_fut = async {
            let r_grant = cons.read_async().await.unwrap();
            r_grant.release(4);

            let time = std::time::Instant::now(); // TODO: Remove time dependence in test
            #[cfg(feature = "verbose")]
            println!("Read completed at {:?}", time);
            time
        };

        let write_fut = async {
            let w_grant = prod.grant_exact_async(3).await.unwrap();
            w_grant.commit(4);

            let time = std::time::Instant::now(); // TODO: Remove time dependence in test
            #[cfg(feature = "verbose")]
            println!("Write completed at {:?}", time);
            time
        };

        let (w_time, r_time) = block_on(join(write_fut, read_fut));
        assert!(r_time < w_time);
    }

    #[test]
    fn grant_exact_loop_too_big() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();
        let w_grant = prod.grant_exact(4).unwrap();
        w_grant.commit(4);

        let read_fut = async {
            let r_grant = cons.read_async().await.unwrap();
            r_grant.release(4);
        };

        let write_fut = async {
            let w_grant = prod.grant_exact_async(4).await;
            assert_eq!(w_grant.unwrap_err(), Error::InsufficientSize);
        };

        block_on(join(write_fut, read_fut));
    }

    #[test]
    fn write_cancelled() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();
        let w_grant_fut = prod.grant_exact_async(6);
        drop(w_grant_fut);
        let r_grant = cons.read();
        assert_eq!(r_grant.unwrap_err(), Error::InsufficientSize);
    }

    #[test]
    fn read_cancelled() {
        let bb: BBBuffer<6> = BBBuffer::new();
        let (mut prod, mut cons) = bb.try_split().unwrap();
        let w_grant = prod.grant_exact(6).unwrap();
        w_grant.commit(6);

        let r_grant_fut = cons.read_async();
        drop(r_grant_fut);

        let w_grant = prod.grant_max_remaining(4);
        assert_eq!(w_grant.unwrap_err(), Error::InsufficientSize);
    }
}
