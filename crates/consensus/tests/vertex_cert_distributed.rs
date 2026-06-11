//! Four StateMachines complete genesis → partials → certs → round 1
//! purely through `step` calls (no network, no host).

use std::collections::VecDeque;

use consensus::{
    Config, Event, HostContext, StateMachine,
    action::Action,
    ports::{
        Clock, DagView, NoPendingBlobs, Persistence, RandomnessBeacon, SignerPort,
        ValidatorSetPort,
    },
};
use crypto::bls::keys::SecretKey;
use types::{
    crypto_types::{BlsSig, Hash32, VrfProof, VrfPubkey},
    primitives::{Epoch, Height, Round, StakeWeight, ValidatorId},
    validator::{ValidatorEntry, ValidatorIdentity, ValidatorSet},
};

struct Ring {
    sks: Vec<SecretKey>,
    set: ValidatorSet,
}

impl Ring {
    fn new(n: u8) -> Self {
        let sks: Vec<SecretKey> = (0..n)
            .map(|i| SecretKey::from_ikm(&[i + 1; 32]).unwrap())
            .collect();
        let entries = (0..n)
            .map(|i| ValidatorEntry {
                id: ValidatorId([i; 32]),
                bls_pubkey: sks[i as usize].public().to_bytes(),
                vrf_pubkey: VrfPubkey::zero(),
                stake: StakeWeight(1),
                identity: ValidatorIdentity {
                    asn: None,
                    cloud: None,
                    region: None,
                },
            })
            .collect();
        Self {
            sks,
            set: ValidatorSet {
                epoch: Epoch(0),
                entries,
                total_stake: StakeWeight(u64::from(n)),
            },
        }
    }
}

struct RingSigner<'a> {
    ring: &'a Ring,
    idx: usize,
}

impl SignerPort for RingSigner<'_> {
    fn sign_bls(&self, dst: &[u8], msg: &[u8]) -> BlsSig {
        crypto::bls::sign::sign(&self.ring.sks[self.idx], dst, msg)
    }
    fn vrf_prove(&self, _alpha: &[u8]) -> consensus::Result<(VrfProof, Hash32)> {
        Ok((VrfProof::zero(), Hash32::zero()))
    }
}

struct FixedValset(ValidatorSet);
impl ValidatorSetPort for FixedValset {
    fn set_for(&self, epoch: Epoch) -> consensus::Result<Option<ValidatorSet>> {
        Ok((self.0.epoch == epoch).then(|| self.0.clone()))
    }
    fn index_of(&self, _e: Epoch, v: &ValidatorId) -> consensus::Result<Option<u32>> {
        Ok(self
            .0
            .entries
            .iter()
            .position(|x| &x.id == v)
            .map(|i| u32::try_from(i).unwrap()))
    }
}

struct EmptyDag;
impl DagView for EmptyDag {
    fn vertex(&self, _h: &Hash32) -> consensus::Result<Option<types::dag::CertifiedVertex>> {
        Ok(None)
    }
    fn vertices_at_round(
        &self,
        _r: Round,
    ) -> consensus::Result<Vec<types::dag::CertifiedVertex>> {
        Ok(vec![])
    }
}

struct ZeroClock;
impl Clock for ZeroClock {
    fn now_nanos(&self) -> u128 {
        0
    }
}

struct ZeroBeacon;
impl RandomnessBeacon for ZeroBeacon {
    fn current(&self) -> consensus::Result<Hash32> {
        Ok(Hash32::zero())
    }
}

struct NoopPersistence;
impl Persistence for NoopPersistence {
    fn store_micro_qc(&self, _q: &types::micro::MicroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn micro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<types::micro::MicroQc>> {
        Ok(None)
    }
    fn store_macro_checkpoint(
        &self,
        _c: &types::macros::MacroCheckpoint,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn store_macro_qc(&self, _q: &types::macros::MacroQc) -> consensus::Result<()> {
        Ok(())
    }
    fn append_slash_evidence(
        &self,
        _e: &types::slashing::SlashEvidence,
    ) -> consensus::Result<()> {
        Ok(())
    }
    fn macro_checkpoint_at(
        &self,
        _h: Height,
    ) -> consensus::Result<Option<types::macros::MacroCheckpoint>> {
        Ok(None)
    }
    fn macro_qc_for(&self, _h: &Hash32) -> consensus::Result<Option<types::macros::MacroQc>> {
        Ok(None)
    }
}

/// Route actions: every Broadcast* becomes the matching Event for all
/// OTHER machines; BroadcastCertifiedVertex also loops back to the
/// sender (orchestrator-loopback semantics).
fn route(
    sender: usize,
    actions: consensus::state_machine::Actions,
    queue: &mut VecDeque<(usize, Event)>,
    n: usize,
) {
    for action in actions {
        match action {
            Action::BroadcastVertexProposal(p) => {
                for i in (0..n).filter(|&i| i != sender) {
                    queue.push_back((i, Event::VertexProposalReceived(p.clone())));
                }
            }
            Action::BroadcastVertexPartial(bp) => {
                for i in (0..n).filter(|&i| i != sender) {
                    queue.push_back((i, Event::VertexPartialReceived(bp.clone())));
                }
            }
            Action::BroadcastCertifiedVertex(cv) => {
                for i in 0..n {
                    queue.push_back((i, Event::CertifiedVertexReceived(cv.clone())));
                }
            }
            _ => {}
        }
    }
}

#[test]
fn four_validators_certify_genesis_and_advance_to_round_one() {
    let n = 4usize;
    let ring = Ring::new(4);
    let valset = FixedValset(ring.set.clone());
    let (dag, clock, beacon, persist, no_pending) =
        (EmptyDag, ZeroClock, ZeroBeacon, NoopPersistence, NoPendingBlobs);

    let mut machines: Vec<StateMachine> = (0..n)
        .map(|i| {
            StateMachine::new(
                Config::default_table_17_1(),
                ValidatorId([u8::try_from(i).unwrap(); 32]),
            )
        })
        .collect();

    let mut queue: VecDeque<(usize, Event)> = VecDeque::new();
    for (i, m) in machines.iter_mut().enumerate() {
        let signer = RingSigner { ring: &ring, idx: i };
        let ctx = HostContext {
            dag: &dag,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let actions = m.genesis_propose(&ctx).unwrap();
        route(i, actions, &mut queue, n);
    }

    let mut steps = 0usize;
    while let Some((i, event)) = queue.pop_front() {
        steps += 1;
        assert!(steps < 10_000, "message storm — protocol not converging");
        let signer = RingSigner { ring: &ring, idx: i };
        let ctx = HostContext {
            dag: &dag,
            clock: &clock,
            valset: &valset,
            beacon: &beacon,
            persistence: &persist,
            signer: &signer,
            pending_blobs: &no_pending,
        };
        let actions = machines[i].step(event, &ctx).unwrap();
        route(i, actions, &mut queue, n);
    }

    for (i, m) in machines.iter().enumerate() {
        assert!(
            m.current_vertex_round() >= 1,
            "validator {i} stuck at round {}",
            m.current_vertex_round()
        );
    }
}
