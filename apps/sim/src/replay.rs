//! Deterministic replay helper.
//!
//! "Replay" in the skeleton phase is identity: the same scenario +
//! validators + rounds + seed produces the same Report. Real event-log
//! replay arrives when scenarios produce traces (plan 03b+).

use crate::{
    args::Args,
    scenarios::{Report, dispatch},
};

/// Run the scenario twice and assert reports are bit-identical.
pub fn assert_deterministic(args: &Args) -> anyhow::Result<()> {
    let a: Report = dispatch(args)?;
    let b: Report = dispatch(args)?;
    let a_json = serde_json::to_string(&a)?;
    let b_json = serde_json::to_string(&b)?;
    anyhow::ensure!(a_json == b_json, "replay diverged");
    Ok(())
}
