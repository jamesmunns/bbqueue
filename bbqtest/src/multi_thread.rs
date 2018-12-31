
#[cfg(test)]
mod tests {
    use bbqueue::BBQueue;
    use std::thread::spawn;

    #[test]
    fn sanity_check() {
        // Hmm, this is probably an interface smell
        let mut bb = Box::new(BBQueue::new());
        let (mut tx, mut rx) = Box::leak(bb).split();

        const ITERS: usize = 100;

        let tx_thr = spawn(move || {
            for i in 0..ITERS {
                'inner: loop {
                    // TODO, max loops? Time?
                    match tx.grant(1) {
                        Ok(gr) => {
                            gr.buf[0] = i as u8;
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
                    // TODO, max loops? Time?
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    // assert_eq!(gr.buf[0], i as u8);
                    eprintln!("{:?}", gr.buf);
                    rx.release(1, gr);
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

}
