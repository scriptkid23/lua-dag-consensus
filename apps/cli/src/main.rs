//! LUA-DAG dev/ops CLI entry point.
#![allow(unreachable_pub)]

use anyhow::Result;
use clap::Parser;

mod args;
mod commands;

fn main() -> Result<()> {
    let cli = args::Cli::parse();
    commands::dispatch(cli.command)
}
