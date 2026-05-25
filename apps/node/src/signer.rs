//! Dev-only local signer for L3 macro paths (plan 03d / 06b-l3).

use std::env;
use std::path::Path;

use consensus::{error::Result, ports::SignerPort};
use crypto::{
    bls::SecretKey,
    hash::{blake3_with_dst, dst},
    vrf::VrfKey,
};
use types::crypto_types::{BlsPubkey, BlsSig, Hash32, VrfProof};

use crate::devnet_keys::{devnet_bls_ikm, devnet_vrf_seed};

/// File- or env-backed signer for a single validator.
pub struct DevSigner {
    bls: SecretKey,
    vrf: VrfKey,
}

impl DevSigner {
    /// Load without valset pubkey check (legacy stub path).
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let label = "node-dev-signer";
        let ikm = resolve_ikm(label, path)?;
        let vrf_seed = vrf_seed_for(label, path.is_some(), &ikm);
        build_signer(ikm, vrf_seed)
    }

    /// Load and verify the derived BLS pubkey matches the valset entry.
    pub fn load_for_label(
        label: &str,
        expected_bls: &BlsPubkey,
        path: Option<&Path>,
    ) -> Result<Self> {
        let ikm = resolve_ikm(label, path)?;
        let vrf_seed = vrf_seed_for(label, path.is_some(), &ikm);
        let signer = build_signer(ikm, vrf_seed)?;
        if signer.bls.public().to_bytes() != *expected_bls {
            return Err(consensus::Error::InvalidConfig(format!(
                "signer BLS pubkey for label `{label}` does not match validator set entry"
            )));
        }
        Ok(signer)
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

fn resolve_ikm(label: &str, path: Option<&Path>) -> Result<[u8; 32]> {
    if let Ok(hex) = env::var("LUA_DAG_BLS_KEY") {
        decode_ikm(&hex)
    } else if let Some(p) = path {
        let raw = std::fs::read_to_string(p).map_err(|e| {
            consensus::Error::InvalidConfig(format!("read signer key: {e}"))
        })?;
        decode_ikm(raw.trim())
    } else {
        Ok(devnet_bls_ikm(label))
    }
}

fn vrf_seed_for(label: &str, key_file: bool, ikm: &[u8; 32]) -> [u8; 32] {
    if env::var("LUA_DAG_BLS_KEY").is_ok() || key_file {
        blake3_with_dst(dst::MACRO_PROPOSER_SIG, ikm).0
    } else {
        devnet_vrf_seed(label)
    }
}

fn build_signer(ikm: [u8; 32], vrf_seed: [u8; 32]) -> Result<DevSigner> {
    let bls = SecretKey::from_ikm(&ikm).map_err(|_| {
        consensus::Error::InvalidConfig("invalid dev BLS key material".into())
    })?;
    Ok(DevSigner {
        bls,
        vrf: VrfKey::from_seed(&vrf_seed),
    })
}

impl std::fmt::Debug for DevSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DevSigner").finish_non_exhaustive()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devnet_keys::devnet_validator_entry;

    #[test]
    fn load_for_label_matches_valset_entry() {
        let entry = devnet_validator_entry("node1");
        let signer = DevSigner::load_for_label("node1", &entry.bls_pubkey, None).unwrap();
        assert_eq!(signer.bls.public().to_bytes(), entry.bls_pubkey);
    }

    #[test]
    fn load_from_temp_key_file() {
        let entry = devnet_validator_entry("node2");
        let ikm = devnet_bls_ikm("node2");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bls.key");
        std::fs::write(&path, hex::encode(ikm)).unwrap();
        let signer = DevSigner::load_for_label("node2", &entry.bls_pubkey, Some(&path)).unwrap();
        assert_eq!(signer.bls.public().to_bytes(), entry.bls_pubkey);
    }

    #[test]
    fn wrong_pubkey_is_rejected() {
        let entry = devnet_validator_entry("node0");
        let err = DevSigner::load_for_label("node3", &entry.bls_pubkey, None).unwrap_err();
        assert!(format!("{err}").contains("does not match"));
    }
}
