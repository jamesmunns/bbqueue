//! Varints
//!
//! This implementation borrows heavily from the `vint64` crate.
//!
//! Below is an example of how prefix bits signal the length of the integer value
//! which follows:
//!
//! | Prefix     | Precision | Total Bytes |
//! |------------|-----------|-------------|
//! | `xxxxxxx1` | 7 bits    | 1 byte      |
//! | `xxxxxx10` | 14 bits   | 2 bytes     |
//! | `xxxxx100` | 21 bits   | 3 bytes     |
//! | `xxxx1000` | 28 bits   | 4 bytes     |
//! | `xxx10000` | 35 bits   | 5 bytes     |
//! | `xx100000` | 42 bits   | 6 bytes     |
//! | `x1000000` | 49 bits   | 7 bytes     |
//! | `10000000` | 56 bits   | 8 bytes     |
//! | `00000000` | 64 bits   | 9 bytes     |

/// Get the length of an encoded `usize` for the given value in bytes.
#[cfg(target_pointer_width = "64")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=7 => 9,
        8..=14 => 8,
        15..=21 => 7,
        22..=28 => 6,
        29..=35 => 5,
        36..=42 => 4,
        43..=49 => 3,
        50..=56 => 2,
        57..=64 => 1,
        _ => {
            // SAFETY:
            //
            // The `leading_zeros` intrinsic returns the number of bits that
            // contain a zero bit. The result will always be in the range of
            // 0..=64 for a 64 bit `usize`, so the above pattern is exhaustive, however
            // it is not exhaustive over the return type of `u32`. Because of
            // this, we mark the "uncovered" part of the match as unreachable
            // for performance reasons.
            #[allow(unsafe_code)]
            unsafe {
                core::hint::unreachable_unchecked()
            }
        }
    }
}

#[cfg(target_pointer_width = "32")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=3 => 5,
        4..=10 => 4,
        11..=17 => 3,
        18..=24 => 2,
        25..=32 => 1,
        _ => {
            // SAFETY:
            //
            // The `leading_zeros` intrinsic returns the number of bits that
            // contain a zero bit. The result will always be in the range of
            // 0..=32 for a 32 bit `usize`, so the above pattern is exhaustive, however
            // it is not exhaustive over the return type of `u32`. Because of
            // this, we mark the "uncovered" part of the match as unreachable
            // for performance reasons.
            #[allow(unsafe_code)]
            unsafe {
                core::hint::unreachable_unchecked()
            }
        }
    }
}

#[cfg(target_pointer_width = "16")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=1 => 3,
        2..=8 => 2,
        9..=16 => 1,
        _ => {
            // SAFETY:
            //
            // The `leading_zeros` intrinsic returns the number of bits that
            // contain a zero bit. The result will always be in the range of
            // 0..=16 for a 16 bit `usize`, so the above pattern is exhaustive, however
            // it is not exhaustive over the return type of `u32`. Because of
            // this, we mark the "uncovered" part of the match as unreachable
            // for performance reasons.
            #[allow(unsafe_code)]
            unsafe {
                core::hint::unreachable_unchecked()
            }
        }
    }
}

#[cfg(target_pointer_width = "8")]
pub fn encoded_len(value: usize) -> usize {
    // I don't think you can have targets with 8 bit pointers in rust,
    // but just in case, 0..=127 would fit in one byte, and 128..=255
    // would fit in two.
    if (value & 0x80) == 0x80 {
        2
    } else {
        1
    }
}

pub fn encode_usize_to_slice(value: usize, length: usize, slice: &mut [u8]) {
    debug_assert!(encoded_len(value) <= length);
    debug_assert!(length <= slice.len());

    let (size, _remainder) = slice.split_at_mut(length);

    if length == 9 {
        // This is possible only with 64-bit usize

        // length byte is zero in this case
        size[0] = 0;
        size[1..9].copy_from_slice(&value.to_le_bytes());
    } else {
        #[cfg(any(target_pointer_width = "64", target_pointer_width = "32"))]
        let encoded = ((value as u64) << 1 | 1) << (length - 1);

        #[cfg(target_pointer_width = "16")]
        let encoded = ((value as u32) << 1 | 1) << (length - 1);

        #[cfg(target_pointer_width = "8")]
        let encoded = ((value as u16) << 1 | 1) << (length - 1);

        size.copy_from_slice(&encoded.to_le_bytes()[..length]);
    }
}

pub fn decoded_len(byte: u8) -> usize {
    byte.trailing_zeros() as usize + 1
}

/// Decode an encoded usize.
///
/// Accepts a reference to a slice containing the encoded usize.
#[cfg(target_pointer_width = "64")]
pub fn decode_usize(input: &[u8]) -> usize {
    let length = decoded_len(input[0]);

    debug_assert!(input.len() >= length);

    let (sz_bytes, _remainder) = input.split_at(length);

    let mut encoded = [0u8; core::mem::size_of::<usize>()];

    // If the target platform is 64 bit, then it is possible
    // to have a total of 9 bytes including the length.
    if length == 9 {
        // 9-byte special case
        encoded.copy_from_slice(&sz_bytes[1..9]);
        usize::from_le_bytes(encoded)
    } else {
        encoded[..length].copy_from_slice(sz_bytes);
        usize::from_le_bytes(encoded) >> length
    }
}

/// Decode an encoded usize.
///
/// Accepts a reference to a slice containing the encoded usize.
#[cfg(not(target_pointer_width = "64"))]
pub fn decode_usize(input: &[u8]) -> usize {
    let length = decoded_len(input[0]);

    debug_assert!(input.len() >= length);

    let (sz_bytes, _remainder) = input.split_at(length);

    let mut encoded = [0u8; core::mem::size_of::<usize>()];

    debug_assert!(length <= encoded.len());
    encoded[..length].copy_from_slice(sz_bytes);
    usize::from_le_bytes(encoded) >> length
}
