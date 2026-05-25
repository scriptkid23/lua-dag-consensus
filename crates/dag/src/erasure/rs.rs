use reed_solomon_erasure::galois_8::ReedSolomon;

use super::config::ErasureConfig;
use super::error::{ErasureError, Result};

/// Pad `payload` to `k * data_shard_size` and produce `n` equal-size shards.
pub fn encode_shards(payload: &[u8], cfg: &ErasureConfig) -> Result<Vec<Vec<u8>>> {
    validate_cfg(cfg)?;
    let padded = pad_to_k_shards(payload, cfg);
    let rs = ReedSolomon::new(
        usize::try_from(cfg.k).expect("k fits usize"),
        usize::try_from(cfg.parity_count()).expect("parity fits usize"),
    )
    .map_err(|e| ErasureError::Codec(e.to_string()))?;

    let mut shards: Vec<Vec<u8>> = padded
        .chunks(cfg.data_shard_size)
        .map(<[u8]>::to_vec)
        .collect();
    shards.resize_with(usize::try_from(cfg.n).expect("n fits usize"), || {
        vec![0u8; cfg.data_shard_size]
    });

    let mut refs: Vec<&mut [u8]> = shards.iter_mut().map(|s| s.as_mut_slice()).collect();
    rs.encode(&mut refs)
        .map_err(|e| ErasureError::Codec(e.to_string()))?;
    Ok(shards)
}

/// Reconstruct the original payload from any `k` (or more) shards.
pub fn decode_shards(
    present: &[(u32, Vec<u8>)],
    cfg: &ErasureConfig,
    orig_len: usize,
) -> Result<Vec<u8>> {
    validate_cfg(cfg)?;
    if present.len() < usize::try_from(cfg.k).expect("k fits usize") {
        return Err(ErasureError::Config("insufficient shards for decode"));
    }

    let rs = ReedSolomon::new(
        usize::try_from(cfg.k).expect("k fits usize"),
        usize::try_from(cfg.parity_count()).expect("parity fits usize"),
    )
    .map_err(|e| ErasureError::Codec(e.to_string()))?;

    let n = usize::try_from(cfg.n).expect("n fits usize");
    let mut shards: Vec<Option<Vec<u8>>> = (0..n).map(|_| None).collect();
    for (index, data) in present {
        let idx = usize::try_from(*index).map_err(|_| ErasureError::Config("bad shard index"))?;
        if idx >= n {
            return Err(ErasureError::Config("shard index out of range"));
        }
        if data.len() != cfg.data_shard_size {
            return Err(ErasureError::Config("unexpected shard length"));
        }
        shards[idx] = Some(data.clone());
    }

    rs.reconstruct(&mut shards)
        .map_err(|e| ErasureError::Codec(e.to_string()))?;

    let mut out = Vec::with_capacity(cfg.padded_len());
    for i in 0..usize::try_from(cfg.k).expect("k fits usize") {
        out.extend_from_slice(shards[i].as_ref().expect("data shard present"));
    }
    out.truncate(orig_len);
    Ok(out)
}

fn validate_cfg(cfg: &ErasureConfig) -> Result<()> {
    if cfg.k == 0 || cfg.n <= cfg.k || cfg.data_shard_size == 0 {
        return Err(ErasureError::Config("k/n/shard_size invalid"));
    }
    Ok(())
}

fn pad_to_k_shards(payload: &[u8], cfg: &ErasureConfig) -> Vec<u8> {
    let mut padded = vec![0u8; cfg.padded_len()];
    let copy_len = payload.len().min(padded.len());
    padded[..copy_len].copy_from_slice(&payload[..copy_len]);
    padded
}
