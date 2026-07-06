/// Fixed `(k, n)` Reed–Solomon parameters for blob erasure (07c).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErasureConfig {
    /// Data shard count.
    pub k: u32,
    /// Total shard count (data + parity).
    pub n: u32,
    /// Byte length of each data/parity shard.
    pub data_shard_size: usize,
}

impl ErasureConfig {
    /// Devnet defaults: 4 data + 4 parity shards (rate 1/2), 32 KiB each.
    #[must_use]
    pub fn devnet_default() -> Self {
        Self {
            k: 4,
            n: 8,
            data_shard_size: 32 * 1024,
        }
    }

    /// Parity shard count (`n - k`).
    #[must_use]
    pub fn parity_count(&self) -> u32 {
        self.n.saturating_sub(self.k)
    }

    /// Padded payload length used for RS encoding.
    #[must_use]
    pub fn padded_len(&self) -> usize {
        usize::try_from(self.k).expect("k fits usize") * self.data_shard_size
    }
}
