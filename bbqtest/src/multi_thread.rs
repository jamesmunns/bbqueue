#[cfg(test)]
mod tests {
    use bbqueue::BBQueue;
    use rand::prelude::*;
    use std::thread::spawn;
    use std::time::{Duration, Instant};

    #[cfg(feature = "travisci")]
    const ITERS: usize = 10_000;
    #[cfg(not(feature = "travisci"))]
    const ITERS: usize = 10_000_000;

    const QUEUE_SIZE: usize = 1024;

    const RPT_IVAL: usize = ITERS / 100;

    const TIMEOUT_NODATA: Duration = Duration::from_millis(10_000);

    #[test]
    fn randomize_tx() {
        #[cfg(feature = "travisci")]
        println!("Hello Travis!");

        println!("RTX: Generating Test Data...");
        let gen_start = Instant::now();
        let mut data = Vec::with_capacity(ITERS);
        (0..ITERS).for_each(|_| data.push(rand::random::<u8>()));
        let mut data_rx = data.clone();

        let mut trng = thread_rng();
        let mut chunks = vec![];
        while !data.is_empty() {
            let chunk_sz = trng.gen_range(1, (QUEUE_SIZE - 1) / 2);
            if chunk_sz > data.len() {
                continue;
            }

            // Note: This gives back data in chunks in reverse order.
            // We later .rev()` this to fix it
            chunks.push(data.split_off(data.len() - chunk_sz));
        }

        // println!("{:?}", chunks);
        // println!("{:?}", data_rx);

        println!("RTX: Generation complete: {:?}", gen_start.elapsed());
        println!("RTX: Running test...");

        let bb = Box::new([0u8; QUEUE_SIZE]);
        let bbl = Box::leak(Box::new(BBQueue::new(Box::leak(bb))));
        let (mut tx, mut rx) = bbl.split();

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();
        let start_time = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            for (i, ch) in chunks.iter().rev().enumerate() {
                let mut semichunk = ch.to_owned();
                // println!("semi: {:?}", semichunk);

                while !semichunk.is_empty() {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        println!("DEADLOCK DUMP, TX: {:?}", unsafe { tx.bbq.as_ref() });
                        panic!("tx timeout, iter {}", i);
                    }

                    'sizer: for sz in (1..(semichunk.len() + 1)).rev() {
                        if let Ok(gr) = tx.grant(sz) {
                            // how do you do this idiomatically?
                            (0..sz).for_each(|idx| {
                                gr.buf[idx] = semichunk.remove(0);
                            });
                            tx.commit(sz, gr);

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += sz;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                                println!("{:?} - rtxtx: {}", start_time.elapsed(), txd_ct);
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
                    // ::std::sync::atomic::fence(::std::sync::atomic::Ordering::SeqCst);
                    if last_rx.elapsed() > TIMEOUT_NODATA {
                        println!("DEADLOCK DUMP, RX: {:?}", unsafe { rx.bbq.as_ref() });
                        panic!("rx timeout, iter {}", i);
                    }
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    // println!("Pop  {}: {:?}", _idx, gr.buf);
                    let act = gr.buf[0] as u8;
                    let exp = i;
                    if act != exp {
                        println!("act: {:?}, exp: {:?}", act, exp);
                        println!("len: {:?}", gr.buf.len());
                        println!("{:?}", gr.buf);
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }
                    rx.release(1, gr);

                    // Update tracking
                    last_rx = Instant::now();
                    rxd_ct += 1;
                    if (rxd_ct / RPT_IVAL) > rxd_ivl {
                        rxd_ivl = rxd_ct / RPT_IVAL;
                        println!("{:?} - rtxrx: {}", start_time.elapsed(), rxd_ct);
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
        // Hmm, this is probably an interface smell
        let bb = Box::new([0u8; QUEUE_SIZE]);
        let bbl = Box::leak(Box::new(BBQueue::new(Box::leak(bb))));
        let panny = format!("{:p}", &bbl.buf[0]);
        let (mut tx, mut rx) = bbl.split();

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();
        let start_time = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            for i in 0..ITERS {
                'inner: loop {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        println!("DEADLOCK DUMP, TX: {:?}", unsafe { tx.bbq.as_ref() });
                        panic!("tx timeout, iter {}", i);
                    }
                    match tx.grant(1) {
                        Ok(gr) => {
                            gr.buf[0] = (i & 0xFF) as u8;
                            tx.commit(1, gr);

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += 1;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                                println!("{:?} - sctx: {}", start_time.elapsed(), txd_ct);
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
                    println!("DEADLOCK DUMP, RX: {:?}", unsafe { rx.bbq.as_ref() });
                    panic!("rx timeout, iter {}", i);
                }

                let gr = rx.read();
                if gr.buf.is_empty() {
                    continue;
                }

                for data in gr.buf {
                    let act = *data;
                    let exp = (i & 0xFF) as u8;
                    if act != exp {
                        println!("baseptr: {}", panny);
                        println!("offendr: {:p}", &gr.buf[0]);
                        println!("act: {:?}, exp: {:?}", act, exp);
                        println!("len: {:?}", gr.buf.len());
                        println!("{:?}", &gr.buf);
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }

                    i += 1;
                }

                rx.release(gr.buf.len(), gr);

                // Update tracking
                last_rx = Instant::now();
                rxd_ct += 1;
                if (rxd_ct / RPT_IVAL) > rxd_ivl {
                    rxd_ivl = rxd_ct / RPT_IVAL;
                    println!("{:?} - scrx: {}", start_time.elapsed(), rxd_ct);
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

    #[test]
    fn sanity_check_grant_max() {
        // Hmm, this is probably an interface smell
        let bb = Box::new([0u8; QUEUE_SIZE]);
        let bbl = Box::leak(Box::new(BBQueue::new(Box::leak(bb))));
        let panny = format!("{:p}", &bbl.buf[0]);
        let (mut tx, mut rx) = bbl.split();

        println!("SCGM: Generating Test Data...");
        let gen_start = Instant::now();

        let mut data_tx = (0..ITERS).map(|i| (i & 0xFF) as u8).collect::<Vec<_>>();
        let mut data_rx = data_tx.clone();

        println!("SCGM: Generated Test Data in: {:?}", gen_start.elapsed());
        println!("SCGM: Starting Test...");

        let mut last_tx = Instant::now();
        let mut last_rx = last_tx.clone();
        let start_time = last_tx.clone();

        let tx_thr = spawn(move || {
            let mut txd_ct = 0;
            let mut txd_ivl = 0;

            while !data_tx.is_empty() {
                'inner: loop {
                    if last_tx.elapsed() > TIMEOUT_NODATA {
                        println!("DEADLOCK DUMP, TX: {:?}", unsafe { tx.bbq.as_ref() });
                        panic!("tx timeout");
                    }
                    match tx.grant_max(6) {
                        // TODO - use bufsize
                        Ok(gr) => {
                            // println!("wrlen: {}", gr.buf.len());
                            let sz = ::std::cmp::min(data_tx.len(), gr.buf.len());
                            for i in 0..sz {
                                gr.buf[i] = data_tx.pop().unwrap();
                            }

                            // Update tracking
                            last_tx = Instant::now();
                            txd_ct += sz;
                            if (txd_ct / RPT_IVAL) > txd_ivl {
                                txd_ivl = txd_ct / RPT_IVAL;
                                println!("{:?} - scgmtx: {}", start_time.elapsed(), txd_ct);
                            }

                            tx.commit(gr.buf.len(), gr);
                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
            // println!("TX Complete");
        });

        let rx_thr = spawn(move || {
            let mut rxd_ct = 0;
            let mut rxd_ivl = 0;

            while !data_rx.is_empty() {
                'inner: loop {
                    if last_rx.elapsed() > TIMEOUT_NODATA {
                        println!("DEADLOCK DUMP, RX: {:?}", unsafe { rx.bbq.as_ref() });
                        panic!("rx timeout");
                    }
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    // println!("rdlen: {}", gr.buf.len());
                    let act = gr.buf[0];
                    let exp = data_rx.pop().unwrap();
                    if act != exp {
                        println!("baseptr: {}", panny);
                        println!("offendr: {:p}", &gr.buf[0]);
                        println!("act: {:?}, exp: {:?}", act, exp);
                        println!("len: {:?}", gr.buf.len());
                        println!("{:?}", gr.buf);
                        panic!("RX Iter: {}");
                    }
                    rx.release(1, gr);

                    // Update tracking
                    last_rx = Instant::now();
                    rxd_ct += 1;
                    if (rxd_ct / RPT_IVAL) > rxd_ivl {
                        rxd_ivl = rxd_ct / RPT_IVAL;
                        println!("{:?} - scgmrx: {}", start_time.elapsed(), rxd_ct);
                    }

                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

}
