//! Scenario dispatch. Each module wires up a `World` and returns a
//! [`Report`] containing checker outcomes + finality-latency stats.

use anyhow::Result;
use serde::Serialize;

use crate::args::{Args, Scenario};

pub mod anchor_dos;
pub mod byzantine_split;
pub mod equivocation_inject;
pub mod happy_path;
pub mod mode_a_subnet;
pub mod mode_b_fallback;
pub mod network_partition;

/// Per-scenario report (JSON-rendered by `main.rs`).
#[derive(Debug, Serialize)]
pub struct Report {
    /// Scenario tag.
    pub scenario: String,
    /// Number of validators.
    pub validators: u32,
    /// Rounds executed.
    pub rounds: u32,
    /// Safety check passed.
    pub safety_ok: bool,
    /// Liveness check passed.
    pub liveness_ok: bool,
    /// `lock_macro` invariant held.
    pub lock_macro_ok: bool,
    /// Optional notes.
    pub notes: Vec<String>,
}

/// Hash the user-supplied seed (hex or arbitrary utf-8) into 32 bytes.
#[must_use]
pub fn parse_seed(s: &str) -> [u8; 32] {
    let trimmed = s.strip_prefix("0x").unwrap_or(s);
    if let Ok(b) = hex::decode(trimmed) {
        if b.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&b);
            return out;
        }
    }
    let mut out = [0u8; 32];
    let h = crypto::hash::blake3_with_dst(crypto::hash::dst::CONTENT_HASH, s.as_bytes());
    out.copy_from_slice(h.as_bytes());
    out
}

/// Dispatch from CLI to the right scenario module.
#[allow(clippy::unnecessary_wraps)]
pub fn dispatch(args: &Args) -> Result<Report> {
    let seed = parse_seed(&args.seed);
    Ok(match args.scenario {
        Scenario::HappyPath => happy_path::run(args.validators, args.rounds, seed),
        Scenario::AnchorDos => anchor_dos::run(args.validators, args.rounds, seed),
        Scenario::ModeBFallback => mode_b_fallback::run(args.validators, args.rounds, seed),
        Scenario::ModeASubnet => mode_a_subnet::run(args.validators, args.rounds, seed),
        Scenario::EquivocationInject => {
            equivocation_inject::run(args.validators, args.rounds, seed)
        }
        Scenario::ByzantineSplit => byzantine_split::run(args.validators, args.rounds, seed),
        Scenario::NetworkPartition => network_partition::run(args.validators, args.rounds, seed),
    })
}
