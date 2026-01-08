#[cfg(test)]
mod tests {
    use bbqueue::{BBBuffer, Error};
    use rand::prelude::*;
    use std::thread::spawn;
    use std::time::{Duration, Instant};

    const ITERS: usize = 10_000_000;

    const RPT_IVAL: usize = ITERS / 100;

    const QUEUE_SIZE: usize = 1024;

    const TIMEOUT_NODATA: Duration = Duration::from_millis(10_000);

    #[test]
    fn randomize_tx() {
        let mut data = Vec::with_capacity(ITERS);
        (0..ITERS).for_each(|_| data.push(rand::random::<u8>()));
        let mut data_rx = data.clone();

        let mut trng = thread_rng();
        let mut chunks = vec![];
        while !data.is_empty() {
            let chunk_sz = trng.gen_range(1..((QUEUE_SIZE / 2) - 1));
            if chunk_sz > data.len() {
                continue;
            }

            // Note: This gives back data in chunks in reverse order.
            // We later .rev()` this to fix it
            chunks.push(data.split_off(data.len() - chunk_sz));
        }

        static BB: BBBuffer<QUEUE_SIZE> = BBBuffer::new();
        let (mut tx, mut rx) = BB.try_split().unwrap();

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            for (i, ch) in chunks.iter().rev().enumerate() {
                let mut semichunk = ch.to_owned();
                // #[cfg(feature = "verbose")] println!("semi: {:?}", semichunk);

                while !semichunk.is_empty() {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        panic!("tx timeout, iter {}", i);
                    }

                    'sizer: for sz in (1..(semichunk.len() + 1)).rev() {
                        if let Ok(mut gr) = tx.grant_exact(sz) {
                            // how do you do this idiomatically?
                            (0..sz).for_each(|idx| {
                                gr[idx] = semichunk.remove(0);
                            });
                            gr.commit(sz);

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += sz;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                            }

                            break 'sizer;
                        }
                    }
                }
            }
        });

        let rx_thr = spawn(move || {
            let mut rxd_ct = 0;
            let mut rxd_ivl = 0;

            for (_idx, i) in data_rx.drain(..).enumerate() {
                'inner: loop {
                    if last_rx.elapsed() > TIMEOUT_NODATA {
                        panic!("rx timeout, iter {}", i);
                    }
                    let gr = match rx.read() {
                        Ok(gr) => gr,
                        Err(Error::InsufficientSize) => continue 'inner,
                        Err(_) => panic!(),
                    };

                    let act = gr[0] as u8;
                    let exp = i;
                    if act != exp {
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }
                    gr.release(1);

                    // Update tracking
                    last_rx = Instant::now();
                    rxd_ct += 1;
                    if (rxd_ct / RPT_IVAL) > rxd_ivl {
                        rxd_ivl = rxd_ct / RPT_IVAL;
                    }

                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

    #[test]
    fn sanity_check() {
        static BB: BBBuffer<QUEUE_SIZE> = BBBuffer::new();
        let (mut tx, mut rx) = BB.try_split().unwrap();

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            for i in 0..ITERS {
                'inner: loop {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        panic!("tx timeout, iter {}", i);
                    }
                    match tx.grant_exact(1) {
                        Ok(mut gr) => {
                            gr[0] = (i & 0xFF) as u8;
                            gr.commit(1);

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += 1;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                            }

                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
        });

        let rx_thr = spawn(move || {
            let mut rxd_ct = 0;
            let mut rxd_ivl = 0;

            let mut i = 0;

            while i < ITERS {
                if last_rx.elapsed() > TIMEOUT_NODATA {
                    panic!("rx timeout, iter {}", i);
                }

                let gr = match rx.read() {
                    Ok(gr) => gr,
                    Err(Error::InsufficientSize) => continue,
                    Err(_) => panic!(),
                };

                for data in &*gr {
                    let act = *data;
                    let exp = (i & 0xFF) as u8;
                    if act != exp {
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }

                    i += 1;
                }

                let len = gr.len();
                rxd_ct += len;
                gr.release(len);

                // Update tracking
                last_rx = Instant::now();
                if (rxd_ct / RPT_IVAL) > rxd_ivl {
                    rxd_ivl = rxd_ct / RPT_IVAL;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

    #[test]
    fn sanity_check_grant_max() {
        static BB: BBBuffer<QUEUE_SIZE> = BBBuffer::new();
        let (mut tx, mut rx) = BB.try_split().unwrap();

        let mut data_tx = (0..ITERS).map(|i| (i & 0xFF) as u8).collect::<Vec<_>>();
        let mut data_rx = data_tx.clone();

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            let mut trng = thread_rng();

            while !data_tx.is_empty() {
                'inner: loop {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        panic!("tx timeout");
                    }
                    match tx.grant_max_remaining(
                        trng.gen_range((QUEUE_SIZE / 3)..((2 * QUEUE_SIZE) / 3)),
                    ) {
                        Ok(mut gr) => {
                            let sz = ::std::cmp::min(data_tx.len(), gr.len());
                            for i in 0..sz {
                                gr[i] = data_tx.pop().unwrap();
                            }

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += sz;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                            }

                            let len = gr.len();
                            gr.commit(len);
                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
        });

        let rx_thr = spawn(move || {
            let mut rxd_ct = 0;
            let mut rxd_ivl = 0;

            while !data_rx.is_empty() {
                'inner: loop {
                    if last_rx.elapsed() > TIMEOUT_NODATA {
                        panic!("rx timeout");
                    }
                    let gr = match rx.read() {
                        Ok(gr) => gr,
                        Err(Error::InsufficientSize) => continue 'inner,
                        Err(_) => panic!(),
                    };

                    let act = gr[0];
                    let exp = data_rx.pop().unwrap();
                    if act != exp {
                        panic!("RX Iter: {}", rxd_ct);
                    }
                    gr.release(1);

                    // Update tracking
                    last_rx = Instant::now();
                    rxd_ct += 1;
                    if (rxd_ct / RPT_IVAL) > rxd_ivl {
                        rxd_ivl = rxd_ct / RPT_IVAL;
                    }

                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }
}
