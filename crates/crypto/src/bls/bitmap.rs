//! Signer bitmaps for aggregated BLS certificates.
//!
//! Bitmap is little-endian-bit, big-endian-byte: bit `i` lives in
//! `bytes[i / 8] >> (i % 8)`. This matches Borsh-friendly slicing.

use crate::error::{Error, Result};

/// Mutable bitmap with a fixed number of slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bitmap {
    bytes: Vec<u8>,
    bits: usize,
}

impl Bitmap {
    /// New all-zero bitmap covering `bits` validators.
    #[must_use]
    pub fn new(bits: usize) -> Self {
        let len = bits.div_ceil(8);
        Self {
            bytes: vec![0u8; len],
            bits,
        }
    }

    /// View raw bytes (length = ⌈bits/8⌉).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Wrap an existing byte vector. `bits` must fit within the buffer.
    pub fn from_bytes(bytes: Vec<u8>, bits: usize) -> Result<Self> {
        let need = bits.div_ceil(8);
        if bytes.len() != need {
            return Err(Error::BitmapLength {
                bitmap_bits: bytes.len() * 8,
                expected: bits,
            });
        }
        Ok(Self { bytes, bits })
    }

    /// Total bit count (validator count).
    #[must_use]
    pub fn len(&self) -> usize {
        self.bits
    }

    /// `true` iff the bitmap covers zero validators.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Number of bits set.
    #[must_use]
    pub fn count_ones(&self) -> usize {
        self.bytes.iter().map(|b| b.count_ones() as usize).sum()
    }

    /// Set bit `i`.
    pub fn set(&mut self, i: usize) -> Result<()> {
        if i >= self.bits {
            return Err(Error::BitmapLength {
                bitmap_bits: self.bits,
                expected: i + 1,
            });
        }
        self.bytes[i / 8] |= 1 << (i % 8);
        Ok(())
    }

    /// Test bit `i`.
    pub fn get(&self, i: usize) -> Result<bool> {
        if i >= self.bits {
            return Err(Error::BitmapLength {
                bitmap_bits: self.bits,
                expected: i + 1,
            });
        }
        Ok(self.bytes[i / 8] & (1 << (i % 8)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_round_trip() {
        let mut b = Bitmap::new(10);
        b.set(0).unwrap();
        b.set(7).unwrap();
        b.set(8).unwrap();
        assert!(b.get(0).unwrap());
        assert!(!b.get(1).unwrap());
        assert!(b.get(7).unwrap());
        assert!(b.get(8).unwrap());
        assert_eq!(b.count_ones(), 3);
    }

    #[test]
    fn out_of_range_errors() {
        let mut b = Bitmap::new(8);
        let err = b.set(8).unwrap_err();
        assert!(matches!(err, Error::BitmapLength { .. }));
    }

    #[test]
    fn from_bytes_validates_length() {
        let err = Bitmap::from_bytes(vec![0; 2], 24).unwrap_err();
        assert!(matches!(err, Error::BitmapLength { .. }));
        let ok = Bitmap::from_bytes(vec![0; 3], 24).unwrap();
        assert_eq!(ok.len(), 24);
    }
}
