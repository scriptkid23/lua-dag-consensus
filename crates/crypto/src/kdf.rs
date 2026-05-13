//! HKDF-style key/value derivation using Blake3 keyed mode.

use blake3::Hasher;

/// HKDF-style expand using Blake3 keyed mode.
///
/// `ikm` is the input keying material (e.g. previous beacon output).
/// `info` is a per-call domain separator. Returns `len` bytes.
#[must_use]
pub fn expand(ikm: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    // Use Blake3 keyed-mode where the key is derived from the IKM, then
    // extend by absorbing `info` plus a counter.
    let key = blake3::hash(ikm);
    let mut out = Vec::with_capacity(len);
    let mut counter: u32 = 0;
    while out.len() < len {
        let mut hasher = Hasher::new_keyed(key.as_bytes());
        hasher.update(info);
        hasher.update(&counter.to_be_bytes());
        let block = hasher.finalize();
        out.extend_from_slice(block.as_bytes());
        counter += 1;
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_is_deterministic() {
        let a = expand(b"ikm", b"info", 64);
        let b = expand(b"ikm", b"info", 64);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn different_info_yields_different_output() {
        let a = expand(b"ikm", b"alpha", 32);
        let b = expand(b"ikm", b"beta", 32);
        assert_ne!(a, b);
    }

    #[test]
    fn supports_long_outputs() {
        let out = expand(b"k", b"i", 200);
        assert_eq!(out.len(), 200);
    }
}
