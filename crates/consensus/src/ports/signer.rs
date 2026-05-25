//! Local validator signing for one `StateMachine::step` call.

use types::crypto_types::{BlsSig, Hash32, VrfProof};

use crate::error::Result;

/// Signs on behalf of the validator that owns this `StateMachine`.
pub trait SignerPort {
    /// BLS sign under the local validator key.
    fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig;

    /// ECVRF prove for macro/L2 sortition alphas.
    fn vrf_prove(&self, alpha: &[u8]) -> Result<(VrfProof, Hash32)>;
}

/// Test/dev stub that panics if signing is invoked.
#[derive(Debug, Default, Clone, Copy)]
pub struct PanickingSigner;

impl SignerPort for PanickingSigner {
    fn sign_bls(&self, _dst: &[u8], _msg: &[u8]) -> BlsSig {
        panic!("SignerPort::sign_bls called without a real signer")
    }

    fn vrf_prove(&self, _alpha: &[u8]) -> Result<(VrfProof, Hash32)> {
        panic!("SignerPort::vrf_prove called without a real signer")
    }
}
