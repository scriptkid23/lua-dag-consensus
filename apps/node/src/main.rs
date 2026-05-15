//! LUA-DAG validator binary entry point.
#![allow(unreachable_pub)] // binary crate: modules are not exported as a library API

mod args;
mod config;
mod observability;
mod orchestrator;
mod rpc_server;
mod runtime;
mod shutdown;
mod timer;
mod validator_set_loader;

fn main() -> anyhow::Result<()> {
    runtime::run()
}
