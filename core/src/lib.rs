//! # BBQueue
//!
//! BBQueue, short for "BipBuffer Queue", is a (work in progress) Single Producer Single Consumer,
//! lockless, no_std, thread safe, queue, based on [BipBuffers].
//!
//! [BipBuffers]: https://www.codeproject.com/Articles/3479/%2FArticles%2F3479%2FThe-Bip-Buffer-The-Circular-Buffer-with-a-Twist
//!
//! It is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
//! systems.
//!
//! While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
//! main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
//! block of contiguous memory, which can be filled (or emptied) by a DMA engine.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "atomic")]
pub mod atomic;

#[cfg(all(feature = "atomic", not(feature = "thumbv6")))]
pub use atomic::*;

#[cfg(feature = "thumbv6")]
pub mod cm_mutex;

#[cfg(all(feature = "thumbv6", not(feature = "atomic")))]
pub use cm_mutex::*;

use core::result::Result as CoreResult;

pub use generic_array::ArrayLength;

/// Result type used by the `BBQueue` interfaces
pub type Result<T> = CoreResult<T, Error>;

/// Error type used by the `BBQueue` interfaces
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Error {
    /// The buffer does not contain sufficient size for the requested action
    InsufficientSize,

    /// Unable to produce another grant, a grant of this type is already in
    /// progress
    GrantInProgress,

    /// Unable to split the buffer, as it has already been split
    AlreadySplit,
}
