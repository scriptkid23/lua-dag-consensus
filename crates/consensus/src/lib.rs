//! LUA-DAG consensus: pure deterministic state machine.
//!
//! This crate contains zero async runtimes, networking, and storage code.
//! All side effects are surfaced as [`Action`](crate::action::Action) values
//! returned by [`StateMachine::step`](crate::state_machine::StateMachine::step).
#![cfg_attr(not(test), warn(missing_docs))]
// Skeleton phase: many handlers are intentionally stubbed.
#![allow(
    clippy::needless_pass_by_value,
    clippy::unused_self,
    clippy::doc_markdown,
    clippy::struct_field_names,
    clippy::unnecessary_wraps,
    clippy::match_same_arms
)]

pub mod action;
pub mod api;
pub mod bullshark;
pub mod config;
pub mod error;
pub mod event;
pub mod host_context;
pub mod l2_minimal;
pub mod leader;
pub mod lock_macro;
pub mod macro_fin;
pub mod ports;
pub mod prelude;
pub mod slashing;
pub mod state_machine;

pub use action::Action;
pub use config::Config;
pub use error::{Error, Result};
pub use event::Event;
pub use host_context::HostContext;
pub use state_machine::StateMachine;
