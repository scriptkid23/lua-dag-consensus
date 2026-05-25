//! Systematic Reed–Solomon encode/decode over GF(256) (07c in-house codec).

use super::config::ErasureConfig;
use super::error::{ErasureError, Result};
use super::gf256;

/// Pad `payload` to `k * data_shard_size` and produce `n` equal-size shards.
pub fn encode_shards(payload: &[u8], cfg: &ErasureConfig) -> Result<Vec<Vec<u8>>> {
    validate_cfg(cfg)?;
    let k = usize::try_from(cfg.k).expect("k fits usize");
    let n = usize::try_from(cfg.n).expect("n fits usize");
    let padded = pad_to_k_shards(payload, cfg);
    let matrix = encoding_matrix(n, k)?;

    let mut shards: Vec<Vec<u8>> = padded
        .chunks(cfg.data_shard_size)
        .map(<[u8]>::to_vec)
        .collect();
    shards.resize_with(n, || vec![0u8; cfg.data_shard_size]);

    for byte in 0..cfg.data_shard_size {
        let data: Vec<u8> = (0..k).map(|row| shards[row][byte]).collect();
        for row in 0..n {
            shards[row][byte] = dot(&matrix[row], &data);
        }
    }
    Ok(shards)
}

/// Reconstruct the original payload from any `k` (or more) shards.
pub fn decode_shards(
    present: &[(u32, Vec<u8>)],
    cfg: &ErasureConfig,
    orig_len: usize,
) -> Result<Vec<u8>> {
    validate_cfg(cfg)?;
    let k = usize::try_from(cfg.k).expect("k fits usize");
    if present.len() < k {
        return Err(ErasureError::Config("insufficient shards for decode"));
    }

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

    reconstruct_shards(&mut shards, cfg)?;

    let mut out = Vec::with_capacity(cfg.padded_len());
    for i in 0..k {
        out.extend_from_slice(shards[i].as_ref().expect("data shard present"));
    }
    out.truncate(orig_len);
    Ok(out)
}

/// Fill every missing shard from any `k` present ones.
fn reconstruct_shards(shards: &mut [Option<Vec<u8>>], cfg: &ErasureConfig) -> Result<()> {
    let k = usize::try_from(cfg.k).expect("k fits usize");
    let n = shards.len();
    let matrix = encoding_matrix(n, k)?;

    let present: Vec<usize> = shards
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.as_ref().map(|_| i))
        .collect();
    if present.len() < k {
        return Err(ErasureError::Config("insufficient shards for decode"));
    }
    let use_rows: Vec<usize> = present.into_iter().take(k).collect();
    let sub = submatrix(&matrix, &use_rows);
    let inv = invert_matrix(&sub).map_err(|e| ErasureError::Codec(e.to_string()))?;

    for byte in 0..cfg.data_shard_size {
        let known: Vec<u8> = use_rows
            .iter()
            .map(|&row| shards[row].as_ref().expect("present")[byte])
            .collect();
        let data = mat_vec_mul(&inv, &known);

        for j in 0..k {
            if shards[j].is_none() {
                shards[j] = Some(vec![0u8; cfg.data_shard_size]);
            }
            shards[j].as_mut().expect("initialized")[byte] = data[j];
        }
        for row in 0..n {
            if shards[row].is_none() {
                shards[row] = Some(vec![0u8; cfg.data_shard_size]);
            }
            shards[row].as_mut().expect("initialized")[byte] = dot(&matrix[row], &data);
        }
    }
    Ok(())
}

/// Build systematic `n × k` encoding matrix (top `k` rows = identity).
fn encoding_matrix(n: usize, k: usize) -> Result<Vec<Vec<u8>>> {
    let v = vandermonde(n, k);
    let top: Vec<Vec<u8>> = v[..k].to_vec();
    let inv = invert_matrix(&top).map_err(|e| ErasureError::Codec(e.to_string()))?;
    Ok(mat_mul(&v, &inv))
}

/// `V[r][c] = r^c` over GF(256), matching `reed-solomon-erasure` row indexing.
fn vandermonde(rows: usize, cols: usize) -> Vec<Vec<u8>> {
    (0..rows)
        .map(|r| {
            let base = u8::try_from(r).expect("row index fits u8");
            (0..cols)
                .map(|c| gf256::pow(base, u32::try_from(c).expect("c fits u32")))
                .collect()
        })
        .collect()
}

fn dot(row: &[u8], col: &[u8]) -> u8 {
    row.iter()
        .zip(col)
        .fold(0u8, |acc, (&a, &b)| gf256::add(acc, gf256::mul(a, b)))
}

fn mat_vec_mul(mat: &[Vec<u8>], vec: &[u8]) -> Vec<u8> {
    mat.iter()
        .map(|row| dot(row, vec))
        .collect()
}

fn mat_mul(a: &[Vec<u8>], b: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let rows = a.len();
    let cols = b[0].len();
    let inner = b.len();
    let mut out = vec![vec![0u8; cols]; rows];
    for i in 0..rows {
        for j in 0..cols {
            let mut sum = 0u8;
            for t in 0..inner {
                sum = gf256::add(sum, gf256::mul(a[i][t], b[t][j]));
            }
            out[i][j] = sum;
        }
    }
    out
}

fn submatrix(mat: &[Vec<u8>], rows: &[usize]) -> Vec<Vec<u8>> {
    rows.iter().map(|&r| mat[r].clone()).collect()
}

fn invert_matrix(mat: &[Vec<u8>]) -> std::result::Result<Vec<Vec<u8>>, &'static str> {
    let n = mat.len();
    if n == 0 || mat.iter().any(|row| row.len() != n) {
        return Err("matrix must be square");
    }

    let mut aug = vec![vec![0u8; 2 * n]; n];
    for i in 0..n {
        for j in 0..n {
            aug[i][j] = mat[i][j];
        }
        aug[i][n + i] = 1;
    }

    for col in 0..n {
        let mut pivot = col;
        while pivot < n && aug[pivot][col] == 0 {
            pivot += 1;
        }
        if pivot >= n {
            return Err("matrix is singular");
        }
        if pivot != col {
            aug.swap(col, pivot);
        }
        let inv_pivot = gf256::inv(aug[col][col]);
        if inv_pivot == 0 {
            return Err("matrix is singular");
        }
        for j in 0..2 * n {
            aug[col][j] = gf256::mul(aug[col][j], inv_pivot);
        }
        for i in 0..n {
            if i == col || aug[i][col] == 0 {
                continue;
            }
            let factor = aug[i][col];
            for j in 0..2 * n {
                aug[i][j] = gf256::add(aug[i][j], gf256::mul(factor, aug[col][j]));
            }
        }
    }

    Ok((0..n).map(|i| aug[i][n..].to_vec()).collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erasure::ErasureConfig;

    #[test]
    fn encoding_matrix_top_is_identity() {
        let matrix = encoding_matrix(6, 4).unwrap();
        for r in 0..4 {
            for c in 0..4 {
                let want = if r == c { 1 } else { 0 };
                assert_eq!(matrix[r][c], want, "({r},{c})");
            }
        }
    }

    #[test]
    fn recovers_from_parity_only_subset() {
        let cfg = ErasureConfig {
            k: 4,
            n: 6,
            data_shard_size: 1024,
        };
        let payload_len = cfg.padded_len() - 96;
        let payload = (0..payload_len as u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        let shards = encode_shards(&payload, &cfg).unwrap();
        let subset: Vec<_> = [4u32, 5, 2, 3]
            .into_iter()
            .map(|idx| (idx, shards[idx as usize].clone()))
            .collect();
        let recovered = decode_shards(&subset, &cfg, payload.len()).unwrap();
        assert_eq!(recovered, payload);
    }
}
