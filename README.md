# BBQueue

[![Documentation](https://docs.rs/bbqueue/badge.svg)](https://docs.rs/bbqueue)

# BBQueue

BBQueue, short for "BipBuffer Queue", is a Single Producer Single Consumer,
lockless, no_std, thread safe, queue, based on [BipBuffers]. For more info on
the design of the lock-free algorithm used by bbqueue, see [this blog post].

[BipBuffers]: https://www.codeproject.com/articles/The-Bip-Buffer-The-Circular-Buffer-with-a-Twist
[this blog post]: https://ferrous-systems.com/blog/lock-free-ring-buffer/

BBQueue is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
systems.

While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
block of contiguous memory, which can be filled (or emptied) by a DMA engine.

## Local usage

```rust
// The "Churrasco" flavor has inline storage, hardware atomic
// support, no async support, and is not reference counted.
use bbqueue::nicknames::Churrasco;

// Create a buffer with six elements
let bb: Churrasco<6> = Churrasco::new();
let prod = bb.stream_producer();
let cons = bb.stream_consumer();

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
use bbqueue::nicknames::Churrasco;
use std::{thread::{sleep, spawn}, time::Duration};

// Create a buffer with six elements
static BB: Churrasco<6> = Churrasco::new();

fn receiver() {
    let cons = BB.stream_consumer();
    loop {
        if let Ok(rgr) = cons.read() {
            assert_eq!(rgr.len(), 1);
            assert_eq!(rgr[0], 123);
            rgr.release(1);
            break;
        }
        // don't do this in real code, use Notify!
        sleep(Duration::from_millis(10));
    }
}

fn main() {
    let prod = BB.stream_producer();

    // spawn the consumer
    let hdl = spawn(receiver);

    // Request space for one byte
    let mut wgr = prod.grant_exact(1).unwrap();

    // Set the data
    wgr[0] = 123;

    assert_eq!(wgr.len(), 1);

    // Make the data ready for consuming
    wgr.commit(1);

    // make sure the receiver terminated
    hdl.join().unwrap();
}
```

## Nicknames

bbqueue uses generics to customize the data structure in four main ways:

* Whether the byte storage is inline (and const-generic), or heap allocated
* Whether the queue is polling-only, or supports async/await sending/receiving
* Whether the queue uses a lock-free algorithm with CAS atomics, or uses a critical section
  (for targets that don't have CAS atomics)
* Whether the queue is reference counted, allowing Producer and Consumer halves to be passed
  around without lifetimes.

See the [`nicknames`](crate::nicknames) module for all sixteen variants.

## Stability

`bbqueue` v0.6 is a breaking change from the older "classic" v0.5 interfaces. The intent is to
have a few minor breaking changes in early 2026, and to get to v1.0 as quickly as possible.


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
