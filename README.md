# BBQueue

BBQueue, short for "BipBuffer Queue", is a (work in progress) Single Producer Single Consumer, lockless, no_std, thread safe, queue, based on BipBuffers. It is written in the Rust Programming Language.

It is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded systems.

While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to main code), you must push the data one piece at a time. With BBQueue, you instead are granted a block of contiguous memory, which can be filled (or emptied) by a DMA engine.

Come back soon for more documentation, better performance, and more examples.

The `bbqueue` crate is located in `core/`, and test are located in `bbqtest/`.

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
