//! Codec + topic round-trip. Real swarm-level round-trip arrives with
//! plan 06 once a complete `node` exists.

use consensus::event::{BlsPartial, SubnetId};
use net::gossip::{Topic, decode_event_payload, encode_action_payload};
use types::{
    crypto_types::{BlsSig, Hash32},
    primitives::ValidatorId,
};

#[test]
fn bls_partial_roundtrips_through_codec() {
    let payload = BlsPartial {
        subnet: SubnetId(3),
        validator: ValidatorId([7; 32]),
        checkpoint_hash: Hash32([1; 32]),
        sig: BlsSig([2; 96]),
    };
    let bytes = encode_action_payload(&payload).unwrap();
    let decoded: BlsPartial = decode_event_payload(&bytes).unwrap();
    assert_eq!(payload, decoded);
}

#[test]
fn topic_for_bls_partial_includes_subnet_id() {
    let t = Topic::BlsPartial(SubnetId(11)).ident();
    assert!(t.to_string().ends_with("/11"));
}
