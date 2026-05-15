//! Subcommand dispatch.

use anyhow::Result;

pub mod bench_aggregate;
pub mod inspect;
pub mod keygen;
pub mod replay_log;
pub mod verify;

use crate::args::Command;

/// Route to the chosen subcommand.
pub fn dispatch(cmd: Command) -> Result<()> {
    match cmd {
        Command::Inspect(a) => inspect::run(&a),
        Command::Keygen(a) => keygen::run(&a),
        Command::Verify(a) => verify::run(&a),
        Command::ReplayLog(a) => replay_log::run(&a),
        Command::BenchAggregate(a) => bench_aggregate::run(&a),
    }
}
