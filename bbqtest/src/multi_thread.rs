
#[cfg(test)]
mod tests {
    use bbqueue::BBQueue;
    use std::thread::spawn;
    use std::time::{Instant, Duration};
    use rand::prelude::*;

    #[test]
    fn randomize_tx() {
        println!("Generating Test Data...");
        const ITERS: usize = 10_000_000;
        let mut data = Vec::with_capacity(ITERS);
        (0..ITERS)
            .for_each(|_| {
                data.push(rand::random::<u8>())
            });
        let mut data_rx = data.clone();

        let mut trng = thread_rng();
        let mut chunks = vec![];
        while !data.is_empty() {
            // NOTE: We must never take chunks larger than 1/2 of
            // the size, otherwise deadlock could happen!
            let chunk_sz = trng.gen_range(1, 4);
            if chunk_sz > data.len() {
                continue;
            }

            // Note: This gives back data in chunks in reverse order.
            // We later .rev()` this to fix it
            chunks.push(data.split_off(data.len()-chunk_sz));
        }

        // println!("{:?}", chunks);
        // println!("{:?}", data_rx);

        eprintln!("Running test...");

        let bb = Box::new(BBQueue::new());
        let bbl = Box::leak(bb);
        let (mut tx, mut rx) = bbl.split();


        let timeout_tx = Duration::from_millis(180_000);
        let timeout_rx = Duration::from_millis(180_100);
        let start_tx = Instant::now();
        let start_rx = start_tx.clone();

        let tx_thr = spawn(move || {
            for (i, ch) in chunks.iter().rev().enumerate() {
                'inner: loop {
                    if start_tx.elapsed() > timeout_tx {
                        panic!("tx timeout, iter {}", i);
                    }

                    match tx.grant(ch.len()) {
                        Ok(gr) => {
                            // println!("Push {}: {:?}", gr.buf.len(), ch);
                            gr.buf.copy_from_slice(&ch);
                            tx.commit(ch.len(), gr);
                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
        });

        let rx_thr = spawn(move || {
            for (_idx, i) in data_rx.drain(..).enumerate() {
                'inner: loop {
                    ::std::sync::atomic::fence(::std::sync::atomic::Ordering::SeqCst);
                    if start_rx.elapsed() > timeout_rx {
                        panic!("rx timeout, iter {}", i);
                    }
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    // println!("{}: {:?}", _idx, gr.buf);
                    let act = gr.buf[0] as u8;
                    let exp = i;
                    if act != exp {
                        println!("act: {:?}, exp: {:?}", act, exp);
                        println!("len: {:?}", gr.buf.len());
                        println!("{:?}", gr.buf);
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }
                    rx.release(1, gr);
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
        let bb = Box::new(BBQueue::new());
        let bbl = Box::leak(bb);
        let panny = format!("{:p}", &bbl.buf[0]);
        let (mut tx, mut rx) = bbl.split();

        const ITERS: usize = 10_000_000;

        let timeout_tx = Duration::from_millis(180_000);
        let timeout_rx = Duration::from_millis(180_100);
        let start_tx = Instant::now();
        let start_rx = start_tx.clone();

        let tx_thr = spawn(move || {
            for i in 0..ITERS {
                'inner: loop {
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
                    ::std::sync::atomic::fence(::std::sync::atomic::Ordering::SeqCst);
                    if start_rx.elapsed() > timeout_rx {
                        panic!("rx timeout, iter {}", i);
                    }
                    let gr = rx.read();
                    if gr.buf.is_empty() {
                        continue 'inner;
                    }
                    let act = gr.buf[0] as u8;
                    let exp = (i & 0xFF) as u8;
                    if act != exp {
                        println!("baseptr: {}", panny);
                        println!("offendr: {:p}", &gr.buf[0]);
                        println!("act: {:?}, exp: {:?}", act, exp);
                        println!("len: {:?}", gr.buf.len());
                        println!("{:?}", gr.buf);
                        panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }
                    rx.release(1, gr);
                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }


    #[test]
    fn sanity_check_grant_max() {
        // Hmm, this is probably an interface smell
        let bb = Box::new(BBQueue::new());
        let bbl = Box::leak(bb);
        let panny = format!("{:p}", &bbl.buf[0]);
        let (mut tx, mut rx) = bbl.split();

        const ITERS: usize = 10_000_000;

        let timeout_tx = Duration::from_millis(180_000);
        let timeout_rx = Duration::from_millis(180_100);
        let start_tx = Instant::now();
        let start_rx = start_tx.clone();

        let mut data_tx = (0..ITERS).map(|i| (i & 0xFF) as u8).collect::<Vec<_>>();
        let mut data_rx = data_tx.clone();

        let tx_thr = spawn(move || {
            while !data_tx.is_empty() {
                'inner: loop {
                    if start_tx.elapsed() > timeout_tx {
                        panic!("tx timeout");
                    }
                    match tx.grant_max(6) { // TODO - use bufsize
                        Ok(gr) => {
                            // println!("grlen: {}", gr.buf.len());
                            for i in 0..::std::cmp::min(data_tx.len(), gr.buf.len()) {
                                gr.buf[i] = data_tx.pop().unwrap();
                            }
                            tx.commit(gr.buf.len(), gr);
                            break 'inner;
                        }
                        Err(_) => {}
                    }
                }
            }
            println!("TX Complete");
        });

        let rx_thr = spawn(move || {
            while !data_rx.is_empty() {
                'inner: loop {
                    if start_rx.elapsed() > timeout_rx {
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
                        // panic!("RX Iter: {}, mod: {}", i, i % 6);
                    }
                    rx.release(1, gr);
                    break 'inner;
                }
            }
        });

        tx_thr.join().unwrap();
        rx_thr.join().unwrap();
    }

}
