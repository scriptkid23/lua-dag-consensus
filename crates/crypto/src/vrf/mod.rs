//! ECVRF (Edwards25519, RFC 9381) wrappers + stake-weighted sortition.

pub mod ecvrf;
pub mod sortition;

pub use ecvrf::{VrfKey, vrf_prove, vrf_verify};
pub use sortition::vrf_to_uniform;
