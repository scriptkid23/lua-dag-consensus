//! `node` library facade for integration tests.
//!
//! The production binary lives in `src/main.rs`; this `lib.rs` simply
//! re-exports the same modules so `tests/*.rs` can drive the runtime via
//! `node::runtime::test_helpers::run_for_test` instead of duplicating
//! startup logic in every test.

#![cfg_attr(not(test), warn(missing_docs))]

pub mod args;
pub mod config;
pub mod config_layers;
pub mod observability;
pub mod orchestrator;
pub mod rpc_server;
pub mod runtime;
pub mod shutdown;
pub mod timer;
pub mod validator_set_loader;
