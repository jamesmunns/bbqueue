# BBQueue

[![Documentation](https://docs.rs/bbqueue/badge.svg)](https://docs.rs/bbqueue)
[![Testing](https://travis-ci.org/jamesmunns/bbqueue.svg?branch=master)](https://travis-ci.org/jamesmunns/bbqueue)

BBQueue, short for "BipBuffer Queue", is a Single Producer Single Consumer,
lockless, no_std, thread safe, queue, based on [BipBuffers]. For more info on
the design of the lock-free algorithm used by bbqueue, see [this blog post].

[BipBuffers]: https://www.codeproject.com/Articles/3479/%2FArticles%2F3479%2FThe-Bip-Buffer-The-Circular-Buffer-with-a-Twist
[this blog post]: https://ferrous-systems.com/blog/lock-free-ring-buffer/

BBQueue is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
systems.

While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
block of contiguous memory, which can be filled (or emptied) by a DMA engine.

## Local usage

```rust
// Create a buffer with six elements
let bb: BBBuffer<U6> = BBBuffer::new();
let (mut prod, mut cons) = bb.try_split().unwrap();

// Request space for one byte
let mut wgr = prod.grant_exact(1).unwrap();

// Set the data
wgr[0] = 123;

assert_eq!(wgr.len(), 1);

// Make the data ready for consuming
wgr.commit(1);

// Read all available bytes
let rgr = cons.read().unwrap();

assert_eq!(rgr[0], 123);

// Release the space for later writes
rgr.release(1);
```

## Static usage

```rust
// Create a buffer with six elements
static BB: BBBuffer<U6> = BBBuffer( ConstBBBuffer::new() );

fn main() {
    // Split the bbqueue into producer and consumer halves.
    // These halves can be sent to different threads or to
    // an interrupt handler for thread safe SPSC usage
    let (mut prod, mut cons) = BB.try_split().unwrap();

    // Request space for one byte
    let mut wgr = prod.grant_exact(1).unwrap();

    // Set the data
    wgr[0] = 123;

    assert_eq!(wgr.len(), 1);

    // Make the data ready for consuming
    wgr.commit(1);

    // Read all available bytes
    let rgr = cons.read().unwrap();

    assert_eq!(rgr[0], 123);

    // Release the space for later writes
    rgr.release(1);

    // The buffer cannot be split twice
    assert!(BB.try_split().is_err());
}
```

The `bbqueue` crate is located in `core/`, and tests are located in `bbqtest/`.

## Features

By default BBQueue uses atomic operations which are available on most platforms. However on some
(mostly embedded) platforms atomic support is limited and with the default features you will get
a compiler error about missing atomic methods.

This crate contains special support for Cortex-M0(+) targets with the `thumbv6` feature. By
enabling the feature, unsupported atomic operations will be replaced with critical sections
implemented by disabling interrupts. The critical sections are very short, a few instructions at
most, so they should make no difference to most applications.

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
