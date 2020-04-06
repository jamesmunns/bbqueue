//! A Framed flavor of BBQueue, useful for variable length packets
//!
//! This module allows for a `Framed` mode of operation,
//! where a size header is included in each grant, allowing for
//! "chunks" of data to be passed through a BBQueue, rather than
//! just a stream of bytes. This is convenient when receiving
//! packets of variable sizes.
//!
//! ## Example
//!
//! ```rust
//! use bbqueue::{BBBuffer, consts::*};
//!
//! let bb: BBBuffer<U1000> = BBBuffer::new();
//! let (mut prod, mut cons) = bb.try_split_framed().unwrap();
//!
//! // One frame in, one frame out
//! let mut wgrant = prod.grant(128).unwrap();
//! assert_eq!(wgrant.len(), 128);
//! for (idx, i) in wgrant.iter_mut().enumerate() {
//!     *i = idx as u8;
//! }
//! wgrant.commit(128);
//!
//! let rgrant = cons.read().unwrap();
//! assert_eq!(rgrant.len(), 128);
//! for (idx, i) in rgrant.iter().enumerate() {
//!     assert_eq!(*i, idx as u8);
//! }
//! rgrant.release();
//! ```
//!
//! ## Frame header
//!
//! An internal header is required for each frame stored
//! inside of the `BBQueue`. This header is never exposed to end
//! users of the bbqueue library.
//!
//! A variable sized integer is used for the header size, and the
//! size of this header is based on the max size requested for the grant.
//! This header size must be factored in when calculating an appropriate
//! total size of your buffer.
//!
//! Even if a smaller portion of the grant is committed, the original
//! requested grant size will be used for the header size calculation.
//!
//! For example, if you request a 128 byte grant, the header size will
//! be two bytes. If you then only commit 32 bytes, two bytes will still
//! be used for the header of that grant.
//!
//! | Grant Size (bytes)    | Header size (bytes)  |
//! | :---                  | :---                 |
//! | 1..(2^7)              | 1                    |
//! | (2^7)..(2^14)         | 2                    |
//! | (2^14)..(2^21)        | 3                    |
//! | (2^21)..(2^28)        | 4                    |
//! | (2^28)..(2^35)        | 5                    |
//! | (2^35)..(2^42)        | 6                    |
//! | (2^42)..(2^49)        | 7                    |
//! | (2^49)..(2^56)        | 8                    |
//! | (2^56)..(2^64)        | 9                    |
//!

// This #[cfg] dance is due to how `bbqueue` automatically re-exports
// `BBBuffer` at the top level if only one feature is selected. This
// should hopefully go away in the next breaking release. For now, if
// both features are selected, we only support the `atomic` variant.
//
// If you would like to use Framed mode with a `thumbv6` device, you
// should first select `default-features = false` in your Cargo.toml.
#[cfg(not(all(feature = "atomic", feature = "thumbv6")))]
use crate::{Consumer, GrantR, GrantW, Producer};

#[cfg(all(feature = "atomic", feature = "thumbv6"))]
use crate::atomic::{Consumer, GrantR, GrantW, Producer};

use crate::{
    vusize::{decode_usize, decoded_len, encode_usize_to_slice, encoded_len},
    Result,
};

use core::{
    cmp::min,
    ops::{Deref, DerefMut},
};
use generic_array::ArrayLength;

/// A producer of Framed data
pub struct FrameProducer<'a, N>
where
    N: ArrayLength<u8>,
{
    pub(crate) producer: Producer<'a, N>,
}

impl<'a, N> FrameProducer<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Receive a grant for a frame with a maximum size of `max_sz` in bytes.
    ///
    /// This size does not include the size of the frame header. The exact size
    /// of the frame can be set on `commit`.
    pub fn grant(&mut self, max_sz: usize) -> Result<FrameGrantW<'a, N>> {
        let hdr_len = encoded_len(max_sz);
        Ok(FrameGrantW {
            grant_w: self.producer.grant_exact(max_sz + hdr_len)?,
            hdr_len,
        })
    }
}

/// A consumer of Framed data
pub struct FrameConsumer<'a, N>
where
    N: ArrayLength<u8>,
{
    pub(crate) consumer: Consumer<'a, N>,
}

impl<'a, N> FrameConsumer<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Obtain the next available frame, if any
    pub fn read(&mut self) -> Option<FrameGrantR<'a, N>> {
        // Get all available bytes. We never wrap a frame around,
        // so if a header is available, the whole frame will be.
        let mut grant_r = self.consumer.read().ok()?;

        // Additionally, we never commit less than a full frame with
        // a header, so if we have ANY data, we'll have a full header
        // and frame. `Consumer::read` will return an Error when
        // there are 0 bytes available.

        // The header consists of a single usize, encoded in native
        // endianess order
        let frame_len = decode_usize(&grant_r);
        let hdr_len = decoded_len(grant_r[0]);
        let total_len = frame_len + hdr_len;

        debug_assert!(grant_r.len() >= total_len);

        // Reduce the grant down to the size of the frame with a header
        grant_r.shrink(total_len);

        Some(FrameGrantR { grant_r, hdr_len })
    }
}

/// A write grant for a single frame
///
/// NOTE: If the grant is dropped without explicitly commiting
/// the contents, then no frame will be comitted for writing.
#[derive(Debug, PartialEq)]
pub struct FrameGrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    grant_w: GrantW<'a, N>,
    hdr_len: usize,
}

/// A read grant for a single frame
///
/// NOTE: If the grant is dropped without explicitly releasing
/// the contents, then no frame will be released.
#[derive(Debug, PartialEq)]
pub struct FrameGrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    grant_r: GrantR<'a, N>,
    hdr_len: usize,
}

impl<'a, N> Deref for FrameGrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.grant_w.buf[self.hdr_len..]
    }
}

impl<'a, N> DerefMut for FrameGrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.grant_w.buf[self.hdr_len..]
    }
}

impl<'a, N> Deref for FrameGrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.grant_r.buf[self.hdr_len..]
    }
}

impl<'a, N> FrameGrantW<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Commit a frame to make it available to the Consumer half.
    ///
    /// `used` is the size of the payload, in bytes, not
    /// including the frame header
    pub fn commit(mut self, used: usize) {
        // Saturate the commit size to the available frame size
        let grant_len = self.grant_w.len();
        let frame_len = min(used, grant_len - self.hdr_len);
        let total_len = frame_len + self.hdr_len;

        // Write the actual frame length to the header
        encode_usize_to_slice(used, self.hdr_len, &mut self.grant_w[..self.hdr_len]);

        // Commit the header + frame
        self.grant_w.commit(total_len);
    }
}

impl<'a, N> FrameGrantR<'a, N>
where
    N: ArrayLength<u8>,
{
    /// Release a frame to make the space available for future writing
    ///
    /// Note: The full frame is always released
    pub fn release(mut self) {
        // For a read grant, we have already shrunk the grant
        // size down to the correct size
        let len = self.grant_r.len();
        self.grant_r.release_inner(len);
    }
}
