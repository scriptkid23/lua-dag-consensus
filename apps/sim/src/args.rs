//! Simulator CLI args.

use clap::{Parser, ValueEnum};

/// LUA-DAG deterministic simulator.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Number of validators (committee size).
    #[arg(long, default_value_t = 4)]
    pub validators: u32,

    /// Number of micro-rounds to simulate.
    #[arg(long, default_value_t = 64)]
    pub rounds: u32,

    /// RNG seed (32-byte hex, or any utf-8 string — hashed to 32 bytes).
    #[arg(long, default_value = "0x00")]
    pub seed: String,

    /// Scenario to run.
    #[arg(long, value_enum, default_value_t = Scenario::HappyPath)]
    pub scenario: Scenario,
}

/// Pre-canned scenarios from spec §8.2.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Scenario {
    /// All validators honest, no faults.
    HappyPath,
    /// 1/3 stake offline for anchor selection.
    AnchorDos,
    /// Macro proposer fails primary + backup → Mode B.
    ModeBFallback,
    /// Mode A subnet aggregation (dev config, ≥8 validators).
    ModeASubnet,
    /// Inject an equivocating validator.
    EquivocationInject,
    /// Four justified macro windows without finalization → leak notification.
    InactivityLeak,
    /// Byzantine split-brain scenario.
    ByzantineSplit,
    /// Network partition + healing.
    NetworkPartition,
}
