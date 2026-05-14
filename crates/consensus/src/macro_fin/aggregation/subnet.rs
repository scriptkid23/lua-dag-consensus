//! Subnet assignment: `subnet(v_i, e) = H(pubkey || R_macro) mod K_e`.

use crypto::hash::{blake3_with_dst, dst};
use types::{crypto_types::Hash32, primitives::ValidatorId};

use super::Ke;

/// Deterministic subnet assignment for a validator at an epoch.
#[derive(Debug)]
pub struct SubnetAssign {
    /// Number of subnets in this epoch.
    pub k_e: Ke,
    /// Beacon output for the macro window.
    pub r_macro: Hash32,
}

impl SubnetAssign {
    /// Subnet index in `0..k_e.0`.
    #[must_use]
    pub fn index_for(&self, validator: &ValidatorId) -> u32 {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(validator.as_bytes());
        buf.extend_from_slice(&self.r_macro.0);
        let h = blake3_with_dst(dst::SUBNET_ASSIGN, &buf);
        let n = u32::from_be_bytes([h.0[0], h.0[1], h.0[2], h.0[3]]);
        n % self.k_e.0.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_assignment_is_deterministic() {
        let assign = SubnetAssign {
            k_e: Ke(8),
            r_macro: Hash32([0xAB; 32]),
        };
        let v = ValidatorId([1; 32]);
        let a = assign.index_for(&v);
        let b = assign.index_for(&v);
        assert_eq!(a, b);
        assert!(a < 8);
    }
}
