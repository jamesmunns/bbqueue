
#[cfg(test)]
mod tests {
    use bbqueue::BBQueue;
    use std::thread::spawn;
    use std::time::{Instant, Duration};

    // AJM: This test hangs/fails!
    #[test]
    fn sanity_check() {
        // Hmm, this is probably an interface smell
        let bb = Box::new(BBQueue::new());
        let (mut tx, mut rx) = Box::leak(bb).split();

        const ITERS: usize = 100000;

        let timeout_tx = Duration::from_millis(100000);
        let timeout_rx = Duration::from_millis(110000);
        let start_tx = Instant::now();
        let start_rx = start_tx.clone();

        let tx_thr = spawn(move || {
            for i in 0..ITERS {
                'inner: loop {
                    // eprintln!("WR: {:?}", unsafe { tx.bbq.as_ref() });
                    if start_tx.elapsed() > timeout_tx {
                        panic!("tx timeout, iter {}", i);
                    }
                    match tx.grant(1) {
                        Ok(gr) => {
                            gr.buf[0] = (i & 0xFF) as u8;
                            tx.commit(1, gr);
                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
        });

        let rx_thr = spawn(move || {
            for i in 0..ITERS {
                'inner: loop {
                    eprintln!("RD: {:?}", unsafe { rx.bbq.as_ref() });
                    if start_rx.elapsed() > timeout_rx {
                        panic!("rx timeout, iter {}", i);
                    }
                    // TODO, max loops? Time?
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    assert_eq!(gr.buf[0], (i & 0xFF) as u8, "RX Iter: {}", i);
                    // eprintln!("{:?}", gr.buf);
                    rx.release(1, gr);
                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

}
