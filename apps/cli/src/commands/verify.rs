//! Verify a Borsh-encoded `SlashEvidence` payload offline.

use anyhow::{Context, Result};
use types::{slashing::SlashEvidence, validator::ValidatorSet};

use crate::args::VerifyArgs;

/// Entrypoint.
pub fn run(args: &VerifyArgs) -> Result<()> {
    let bytes = std::fs::read(&args.evidence).context("read evidence file")?;
    let ev: SlashEvidence =
        borsh::from_slice(&bytes).map_err(|e| anyhow::anyhow!("decode SlashEvidence: {e}"))?;
    let raw = std::fs::read_to_string(&args.valset).context("read valset file")?;
    let set: ValidatorSet = toml::from_str(&raw).context("parse valset TOML")?;
    consensus::slashing::verify_evidence(&ev, &set).context("consensus verifier")?;
    println!("evidence-ok kind={}", kind_of(&ev));
    Ok(())
}

fn kind_of(ev: &SlashEvidence) -> &'static str {
    match ev {
        SlashEvidence::MacroEquivocation(_) => "macro_equivocation",
        SlashEvidence::Surround(_) => "surround",
        SlashEvidence::DoubleVote(_) => "double_vote",
        SlashEvidence::VertexEquivocation(_) => "vertex_equivocation",
    }
}
