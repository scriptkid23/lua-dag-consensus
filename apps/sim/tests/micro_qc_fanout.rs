//! `VirtualNet` maps `BroadcastMicroQc` to peer `MicroQcAssembled` deliveries.

use consensus::{action::Action, event::Event};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sim::virtual_net::VirtualNet;
use types::{
    crypto_types::{BlsAggSig, BlsSig, Hash32},
    micro::MicroQc,
};

fn fixture_micro_qc() -> MicroQc {
    MicroQc {
        checkpoint_hash: Hash32([1; 32]),
        agg: BlsAggSig {
            sig: BlsSig([0xAB; 96]),
            bitmap: vec![0xFF],
        },
    }
}

#[test]
fn broadcast_micro_qc_reaches_other_validators() {
    let mut net = VirtualNet::new();
    let qc = fixture_micro_qc();
    let action = Action::BroadcastMicroQc(qc);
    let mut rng = ChaCha20Rng::from_seed([9; 32]);
    net.enqueue_from_action(0, &action, 4, 0, &mut rng);
    let msgs = net.drain_due(0);
    assert_eq!(msgs.len(), 3);
    assert!(matches!(msgs[0].event, Event::MicroQcAssembled(_)));
}
