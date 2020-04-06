///! Varints
///!
///! This implementation borrows heavily from the `vint64` crate.

/// Get the length of an encoded `vint64` for the given value in bytes.
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
            // 0..=64 for a `u64`, so the above pattern is exhaustive, however
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
            // 0..=u32 for a `uu32`, so the above pattern is exhaustive, however
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
            // 0..=u16 for a `u16`, so the above pattern is exhaustive, however
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
    if (value & 0x80) == 0x80 {
        2
    } else {
        1
    }
}

pub fn encode_usize_to_slice(value: usize, length: usize, slice: &mut [u8]) {
    debug_assert!(encoded_len(value) <= length);
    debug_assert!(length >= slice.len());

    let (size, _remainder) = slice.split_at_mut(length);

    #[cfg(target_pointer_width = "64")]
    if length == 9 {
        // length byte is zero in this case
        size[0] = 0;
        size[1..9].copy_from_slice(&value.to_le_bytes());
    } else {
        let encoded = (value << 1 | 1) << (length - 1);
        size.copy_from_slice(&encoded.to_le_bytes()[..length]);
    }

    #[cfg(not(target_pointer_width = "64"))]
    {
        let encoded = (value << 1 | 1) << (length - 1);
        size.copy_from_slice(&encoded.to_le_bytes()[..length]);
    }
}

pub fn decoded_len(byte: u8) -> usize {
    byte.trailing_zeros() as usize + 1
}

/// Decode an encoded usize.
///
/// Accepts a reference to a slice containing the encoded usize.
pub fn decode_usize(input: &[u8]) -> usize {
    let length = decoded_len(input[0]);

    debug_assert!(input.len() >= length);

    let (sz_bytes, _remainder) = input.split_at(length);

    let mut encoded = [0u8; core::mem::size_of::<usize>()];

    // If the target platform is 64 bit, then it is possible
    // to have a total of 9 bytes including the length.
    #[cfg(target_pointer_width = "64")]
    if length == 9 {
        // 9-byte special case
        encoded.copy_from_slice(&sz_bytes[1..9]);
        usize::from_le_bytes(encoded)
    } else {
        encoded[..length].copy_from_slice(sz_bytes);
        usize::from_le_bytes(encoded) >> length
    }

    #[cfg(not(target_pointer_width = "64"))]
    {
        debug_assert!(length <= encoded.len());
        encoded[..length].copy_from_slice(sz_bytes);
        usize::from_le_bytes(encoded) >> length
    }
}
