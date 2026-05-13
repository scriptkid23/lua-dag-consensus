//! BLS12-381 over `blst`.

pub mod aggregate;
pub mod bitmap;
pub mod keys;
pub mod sign;

pub use aggregate::{aggregate_sigs, verify_aggregate};
pub use bitmap::Bitmap;
pub use keys::{PublicKey, SecretKey, generate_pop, verify_pop};
pub use sign::{sign, verify};
