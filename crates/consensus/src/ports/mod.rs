//! Outbound dependency-injection traits.
//!
//! `consensus` calls these to read the outside world; host binaries
//! provide concrete impls (`storage`, `net`, `node::timer`, …).

pub mod clock;
pub mod dag_view;
pub mod persistence;
pub mod rng_beacon;
pub mod validator_set;

pub use clock::Clock;
pub use dag_view::DagView;
pub use persistence::Persistence;
pub use rng_beacon::RandomnessBeacon;
pub use validator_set::ValidatorSetPort;
