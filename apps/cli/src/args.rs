//! Top-level argument parser.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// LUA-DAG developer + operator tool.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None, name = "lua-dag")]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// All subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Decode + dump artifacts from a `RocksDB` directory.
    Inspect(InspectArgs),
    /// Generate a fresh BLS + VRF key bundle.
    Keygen(KeygenArgs),
    /// Verify a slashing-evidence payload offline.
    Verify(VerifyArgs),
    /// Replay a Borsh-encoded `Event` log against a fresh state machine.
    ReplayLog(ReplayArgs),
    /// Ad-hoc BLS aggregate-throughput benchmark.
    BenchAggregate(BenchArgs),
}

/// `inspect` args.
#[derive(Debug, clap::Args)]
pub struct InspectArgs {
    /// Path to the `RocksDB` directory.
    #[arg(long)]
    pub db: PathBuf,
    /// What to dump.
    #[arg(long, value_enum, default_value_t = InspectKind::Summary)]
    pub kind: InspectKind,
}

/// What to inspect.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum InspectKind {
    /// One-line summary per column family.
    Summary,
    /// Dump every `MacroCheckpoint` (`height`, `hash`).
    MacroCheckpoints,
    /// Dump every `MacroQc` (`checkpoint_hash`, `mode`).
    MacroQcs,
}

/// `keygen` args.
#[derive(Debug, clap::Args)]
pub struct KeygenArgs {
    /// 32-byte hex seed; omit for random.
    #[arg(long)]
    pub seed: Option<String>,
}

/// `verify` args.
#[derive(Debug, clap::Args)]
pub struct VerifyArgs {
    /// Path to a Borsh-encoded `SlashEvidence` payload.
    #[arg(long)]
    pub evidence: PathBuf,
    /// Validator set snapshot (TOML) for signature verification.
    #[arg(long)]
    pub valset: PathBuf,
}

/// `replay-log` args.
#[derive(Debug, clap::Args)]
pub struct ReplayArgs {
    /// Path to a Borsh-encoded `Vec<Event>` payload.
    #[arg(long)]
    pub log: PathBuf,
}

/// `bench-aggregate` args.
#[derive(Debug, clap::Args)]
pub struct BenchArgs {
    /// Number of partials to aggregate.
    #[arg(long, default_value_t = 100)]
    pub partials: u32,
    /// Number of repetitions.
    #[arg(long, default_value_t = 10)]
    pub iters: u32,
}
