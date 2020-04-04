//! # BBQueue
//!
//! BBQueue, short for "BipBuffer Queue", is a Single Producer Single Consumer,
//! lockless, no_std, thread safe, queue, based on [BipBuffers]. For more info on
//! the design of the lock-free algorithm used by bbqueue, see [this blog post].
//!
//! [BipBuffers]: https://www.codeproject.com/Articles/3479/%2FArticles%2F3479%2FThe-Bip-Buffer-The-Circular-Buffer-with-a-Twist
//! [this blog post]: https://ferrous-systems.com/blog/lock-free-ring-buffer/
//!
//! BBQueue is designed (primarily) to be a First-In, First-Out queue for use with DMA on embedded
//! systems.
//!
//! While Circular/Ring Buffers allow you to send data between two threads (or from an interrupt to
//! main code), you must push the data one piece at a time. With BBQueue, you instead are granted a
//! block of contiguous memory, which can be filled (or emptied) by a DMA engine.
//!
//! ## Local usage
//!
//! ```rust, no_run
//! # #[cfg(feature = "atomic")]
//! # use bbqueue::atomic::{BBBuffer, consts::*};
//! # #[cfg(not(feature = "atomic"))]
//! # use bbqueue::cm_mutex::{BBBuffer, consts::*};
//! #
//! // Create a buffer with six elements
//! let bb: BBBuffer<U6> = BBBuffer::new();
//! let (mut prod, mut cons) = bb.try_split().unwrap();
//!
//! // Request space for one byte
//! let mut wgr = prod.grant_exact(1).unwrap();
//!
//! // Set the data
//! wgr[0] = 123;
//!
//! assert_eq!(wgr.len(), 1);
//!
//! // Make the data ready for consuming
//! wgr.commit(1);
//!
//! // Read all available bytes
//! let rgr = cons.read().unwrap();
//!
//! assert_eq!(rgr[0], 123);
//!
//! // Release the space for later writes
//! rgr.release(1);
//! ```
//!
//! ## Static usage
//!
//! ```rust, no_run
//! # #[cfg(feature = "atomic")]
//! # use bbqueue::atomic::{BBBuffer, ConstBBBuffer, consts::*};
//! # #[cfg(not(feature = "atomic"))]
//! # use bbqueue::cm_mutex::{BBBuffer, ConstBBBuffer, consts::*};
//! #
//! // Create a buffer with six elements
//! static BB: BBBuffer<U6> = BBBuffer( ConstBBBuffer::new() );
//!
//! fn main() {
//!     // Split the bbqueue into producer and consumer halves.
//!     // These halves can be sent to different threads or to
//!     // an interrupt handler for thread safe SPSC usage
//!     let (mut prod, mut cons) = BB.try_split().unwrap();
//!
//!     // Request space for one byte
//!     let mut wgr = prod.grant_exact(1).unwrap();
//!
//!     // Set the data
//!     wgr[0] = 123;
//!
//!     assert_eq!(wgr.len(), 1);
//!
//!     // Make the data ready for consuming
//!     wgr.commit(1);
//!
//!     // Read all available bytes
//!     let rgr = cons.read().unwrap();
//!
//!     assert_eq!(rgr[0], 123);
//!
//!     // Release the space for later writes
//!     rgr.release(1);
//!
//!     // The buffer cannot be split twice
//!     assert!(BB.try_split().is_err());
//! }
//! ```

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

pub mod framed;

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
