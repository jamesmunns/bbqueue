#[cfg(test)]
mod tests {
    use bbqueue::BBBuffer;
    use futures::executor::block_on;

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
}