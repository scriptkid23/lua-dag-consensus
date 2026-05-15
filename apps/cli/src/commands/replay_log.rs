//! Replay a Borsh-encoded `Vec<Event>` against a fresh state machine.

use anyhow::{Context, Result};
use consensus::{Config, StateMachine, event::Event};

use crate::args::ReplayArgs;

/// Entrypoint.
pub fn run(args: &ReplayArgs) -> Result<()> {
    let bytes = std::fs::read(&args.log).context("read event log")?;
    let events: Vec<Event> =
        borsh::from_slice(&bytes).map_err(|e| anyhow::anyhow!("decode Vec<Event>: {e}"))?;

    let mut sm = StateMachine::new(Config::default_table_17_1());
    let mut total_actions = 0usize;
    for (i, ev) in events.into_iter().enumerate() {
        let actions = sm.step(ev).with_context(|| format!("step #{i}"))?;
        total_actions += actions.len();
    }
    println!("replay-ok actions_emitted={total_actions}");
    Ok(())
}
