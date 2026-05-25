//! End-to-end BLS sign/verify + aggregate threshold for L3.

use std::collections::{BTreeSet, HashMap};

use consensus::{
    macro_fin::{macro_qc::try_finalize_mode0, messages, proposer::vrf_alpha, verify},
    ports::SignerPort,
};
use crypto::hash::dst;
use types::{
    crypto_types::{Hash32, VrfPubkey},
    macros::{MacroCheckpoint, MacroProposal},
    primitives::{Epoch, Height, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

struct TestRing {
    bls: Vec<crypto::bls::SecretKey>,
    vrf: Vec<crypto::vrf::VrfKey>,
}

impl TestRing {
    fn new(n: u32) -> Self {
        let seed = [0x44; 32];
        let mut bls = Vec::new();
        let mut vrf = Vec::new();
        for i in 0..n {
            let mut label = [0u8; 36];
            label[..32].copy_from_slice(&seed);
            label[32..].copy_from_slice(&i.to_be_bytes());
            bls.push(
                crypto::bls::SecretKey::from_ikm(
                    &crypto::hash::blake3_with_dst(dst::VALIDATOR_BLS_PARTIAL, &label).0,
                )
                .unwrap(),
            );
            vrf.push(crypto::vrf::VrfKey::from_seed(
                &crypto::hash::blake3_with_dst(dst::MACRO_PROPOSER_SIG, &label).0,
            ));
        }
        Self { bls, vrf }
    }

    fn set(&self, n: u32) -> ValidatorSet {
        let entries = (0..n)
            .map(|i| {
                let mut id = [0u8; 32];
                id[..4].copy_from_slice(&i.to_be_bytes());
                ValidatorEntry {
                    id: ValidatorId(id),
                    bls_pubkey: self.bls[i as usize].public().to_bytes(),
                    vrf_pubkey: VrfPubkey(self.vrf[i as usize].pubkey()),
                    stake: StakeWeight(1),
                    identity: ValidatorIdentity {
                        asn: None,
                        cloud: None,
                        region: None,
                    },
                }
            })
            .collect();
        ValidatorSet {
            epoch: Epoch(0),
            entries,
            total_stake: StakeWeight(u64::from(n)),
        }
    }
}

struct Signer<'a> {
    ring: &'a TestRing,
    idx: usize,
}

impl SignerPort for Signer<'_> {
    fn sign_bls(&self, d: &[u8], msg: &[u8]) -> types::crypto_types::BlsSig {
        crypto::bls::sign::sign(&self.ring.bls[self.idx], d, msg)
    }

    fn vrf_prove(
        &self,
        alpha: &[u8],
    ) -> consensus::Result<(types::crypto_types::VrfProof, Hash32)> {
        Ok(crypto::vrf::vrf_prove(&self.ring.vrf[self.idx], alpha))
    }
}

#[test]
fn four_validators_threshold_aggregate_verifies() {
    let ring = TestRing::new(4);
    let set = ring.set(4);
    let cp = MacroCheckpoint {
        height: Height(0),
        epoch: Epoch(0),
        parent: Hash32::zero(),
        micro_root: Hash32([0x11; 32]),
        hash: Hash32([0x22; 32]),
    };
    let mut partial_sigs = HashMap::new();
    let mut signers = BTreeSet::new();
    for idx in [0usize, 2, 3] {
        let id = set.entries[idx].id;
        signers.insert(id);
        let msg = messages::checkpoint_message(&cp);
        partial_sigs.insert(
            (cp.hash, id),
            Signer { ring: &ring, idx }.sign_bls(dst::MACRO_CHECKPOINT, &msg),
        );
    }
    let qc = try_finalize_mode0(cp.hash, &signers, &partial_sigs, &set, &cp).unwrap();
    assert!(verify::verify_macro_qc(&set, &qc, &cp));

    let proposer = set.entries[0].id;
    let beacon = Hash32([0xBE; 32]);
    let alpha = vrf_alpha(&beacon, Height(0), &proposer);
    let signer = Signer { ring: &ring, idx: 0 };
    let (vrf_proof, _) = signer.vrf_prove(&alpha).unwrap();
    let proposal = MacroProposal {
        checkpoint: cp.clone(),
        proposer,
        vrf_proof,
        proposer_sig: signer.sign_bls(
            dst::MACRO_PROPOSER_SIG,
            &messages::proposer_message(&proposer, &cp),
        ),
    };
    assert!(verify::verify_proposal(&set, &proposal, &alpha));
}
