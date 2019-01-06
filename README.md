# BBQueue

BBQueue, short for "BipBuffer Queue", is a (work in progress) Single Producer Single Consumer, lockless, no_std, thread safe, queue, based on BipBuffers. It is written in the Rust Programming Language.

It is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded systems.

While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to main code), you must push the data one piece at a time. With BBQueue, you instead are granted a block of contiguous memory, which can be filled (or emptied) by a DMA engine.

## Using in a single threaded context

```rust
use bbqueue::{BBQueue, bbq};

fn main() {
    // Create a statically allocated instance
    let bbq = bbq!(1024).unwrap();

    // Obtain a write grant of size 128 bytes
    let wgr = bbq.grant(128).unwrap();

    // Fill the buffer with data
    wgr.buf.copy_from_slice(&[0xAFu8; 128]);

    // Commit the write, to make the data available to be read
    bbq.commit(wgr.buf.len(), wgr);

    // Obtain a read grant of all available and contiguous bytes
    let rgr = bbq.read().unwrap();

    for i in 0..128 {
        assert_eq!(rgr.buf[i], 0xAFu8);
    }

    // Release the bytes, allowing the space
    // to be re-used for writing
    bbq.release(rgr.buf.len(), rgr);
}
```

## Using in a multi-threaded environment (or with interrupts, etc.)

```rust
use bbqueue::{BBQueue, bbq};
use std::thread::spawn;

fn main() {
    // Create a statically allocated instance
    let bbq = bbq!(1024).unwrap();
    let (mut tx, mut rx) = bbq.split();

    let txt = spawn(move || {
        for tx_i in 0..128 {
            'inner: loop {
                match tx.grant(4) {
                    Ok(gr) => {
                        gr.buf.copy_from_slice(&[tx_i as u8; 4]);
                        tx.commit(4, gr);
                        break 'inner;
                    }
                    _ => {}
                }
            }
        }
    });

    let rxt = spawn(move || {
        for rx_i in 0..128 {
            'inner: loop {
                match rx.read() {
                    Ok(gr) => {
                        if gr.buf.len() < 4 {
                            rx.release(0, gr);
                            continue 'inner;
                        }

                        assert_eq!(&gr.buf[..4], &[rx_i as u8; 4]);
                        rx.release(4, gr);
                        break 'inner;
                    }
                    _ => {}
                }
            }
        }
    });

    txt.join().unwrap();
    rxt.join().unwrap();
}
```

The `bbqueue` crate is located in `core/`, and tests are located in `bbqtest/`.

# License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
