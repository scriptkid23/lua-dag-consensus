//! LUA-DAG deterministic adversarial simulator entry point.

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = sim::args::Args::parse();
    let report = sim::scenarios::dispatch(&args)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
