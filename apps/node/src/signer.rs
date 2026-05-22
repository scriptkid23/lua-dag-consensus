//! Dev-only local signer for L3 macro paths (plan 03d).

use std::env;
use std::path::Path;

use consensus::{error::Result, ports::SignerPort};
use crypto::{
    bls::SecretKey,
    hash::{blake3_with_dst, dst},
    vrf::VrfKey,
};
use types::crypto_types::{BlsSig, Hash32, VrfProof};

/// File- or env-backed signer for a single validator.
pub struct DevSigner {
    bls: SecretKey,
    vrf: VrfKey,
}

impl DevSigner {
    /// Load from `LUA_DAG_BLS_KEY` (32-byte hex) or `path` when set.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let ikm = if let Ok(hex) = env::var("LUA_DAG_BLS_KEY") {
            decode_ikm(&hex)?
        } else if let Some(p) = path {
            let raw = std::fs::read_to_string(p).map_err(|e| {
                consensus::Error::InvalidConfig(format!("read signer key: {e}"))
            })?;
            decode_ikm(raw.trim())?
        } else {
            let seed = blake3_with_dst(dst::DEVNET_PEER_IDENTITY, b"node-dev-signer");
            seed.0
        };
        let bls = SecretKey::from_ikm(&ikm).map_err(|_| {
            consensus::Error::InvalidConfig("invalid dev BLS key material".into())
        })?;
        let vrf_seed = blake3_with_dst(dst::MACRO_PROPOSER_SIG, &ikm);
        Ok(Self {
            bls,
            vrf: VrfKey::from_seed(&vrf_seed.0),
        })
    }
}

impl SignerPort for DevSigner {
    fn sign_bls(&self, dst_tag: &[u8], msg: &[u8]) -> BlsSig {
        crypto::bls::sign::sign(&self.bls, dst_tag, msg)
    }

    fn vrf_prove(&self, alpha: &[u8]) -> Result<(VrfProof, Hash32)> {
        Ok(crypto::vrf::vrf_prove(&self.vrf, alpha))
    }
}

fn decode_ikm(hex: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex.trim()).map_err(|e| {
        consensus::Error::InvalidConfig(format!("decode LUA_DAG_BLS_KEY hex: {e}"))
    })?;
    if bytes.len() != 32 {
        return Err(consensus::Error::InvalidConfig(
            "BLS key must be 32 bytes".into(),
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
