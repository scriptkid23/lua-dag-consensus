//! Generate a BLS + VRF key bundle (+ Proof-of-Possession).

use anyhow::{Context, Result};
use crypto::{
    bls::{SecretKey, generate_pop},
    vrf::VrfKey,
};
use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::Serialize;

use crate::args::KeygenArgs;

#[derive(Serialize)]
#[allow(clippy::struct_field_names)]
struct KeyBundle {
    bls_pubkey_hex: String,
    bls_pop_hex: String,
    vrf_pubkey_hex: String,
    seed_hex: String,
}

/// Entrypoint.
pub fn run(args: &KeygenArgs) -> Result<()> {
    let seed = if let Some(s) = &args.seed {
        let trimmed = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(trimmed).context("seed must be 32-byte hex")?;
        anyhow::ensure!(bytes.len() == 32, "seed must be exactly 32 bytes");
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        out
    } else {
        let mut out = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut out);
        out
    };

    let mut rng = ChaCha20Rng::from_seed(seed);
    let sk = SecretKey::random(&mut rng).context("BLS keygen")?;
    let pop = generate_pop(&sk);
    let vrf = VrfKey::from_seed(&seed);

    let bundle = KeyBundle {
        bls_pubkey_hex: hex::encode(sk.public().to_bytes().0),
        bls_pop_hex: hex::encode(pop.0.0),
        vrf_pubkey_hex: hex::encode(vrf.pubkey()),
        seed_hex: hex::encode(seed),
    };
    println!("{}", serde_json::to_string_pretty(&bundle)?);
    Ok(())
}
