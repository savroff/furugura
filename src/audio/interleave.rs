//! Stereo interleaving for two mono channels.
//!
//! Convention used everywhere else in the codebase (transcript merge,
//! diarization-prior assumptions): **left = mic ("me"), right = system
//! ("everyone else")**.

/// Interleave two equal-length mono i16 slices into stereo.
/// `out` is reused (cleared) so callers can amortize allocation.
pub fn interleave_stereo_i16(left: &[i16], right: &[i16], out: &mut Vec<i16>) {
    debug_assert_eq!(left.len(), right.len());
    out.clear();
    out.reserve(left.len() * 2);
    for (l, r) in left.iter().zip(right.iter()) {
        out.push(*l);
        out.push(*r);
    }
}

/// Decode a slice of little-endian s16 bytes into i16 samples.
/// Returns the number of *complete* samples written; trailing odd byte (if any)
/// is left for the caller's next read to combine with.
pub fn decode_le_s16(bytes: &[u8], out: &mut Vec<i16>) -> usize {
    let pairs = bytes.len() / 2;
    out.clear();
    out.reserve(pairs);
    for i in 0..pairs {
        let lo = bytes[i * 2];
        let hi = bytes[i * 2 + 1];
        out.push(i16::from_le_bytes([lo, hi]));
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleave_basic() {
        let left = [1i16, 3, 5, 7];
        let right = [2i16, 4, 6, 8];
        let mut out = Vec::new();
        interleave_stereo_i16(&left, &right, &mut out);
        assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn interleave_clears_existing() {
        let left = [10i16, 20];
        let right = [11i16, 21];
        let mut out = vec![999, 999, 999];
        interleave_stereo_i16(&left, &right, &mut out);
        assert_eq!(out, vec![10, 11, 20, 21]);
    }

    #[test]
    fn decode_le_s16_roundtrips() {
        let samples = [0i16, 1, -1, i16::MAX, i16::MIN, 12345];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        let n = decode_le_s16(&bytes, &mut out);
        assert_eq!(n, samples.len());
        assert_eq!(out, samples.to_vec());
    }

    #[test]
    fn decode_le_s16_handles_empty() {
        let mut out = Vec::new();
        assert_eq!(decode_le_s16(&[], &mut out), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn decode_le_s16_drops_trailing_odd_byte() {
        // 5 bytes = 2 complete samples (4 bytes) + 1 leftover.
        let bytes = [0x01, 0x00, 0x02, 0x00, 0xff];
        let mut out = Vec::new();
        assert_eq!(decode_le_s16(&bytes, &mut out), 2);
        assert_eq!(out, vec![1, 2]);
    }
}
