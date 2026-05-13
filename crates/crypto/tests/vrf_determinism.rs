//! VRF determinism + cross-key isolation.

use crypto::vrf::{VrfKey, vrf_prove, vrf_verify};

#[test]
fn same_alpha_same_key_yields_same_proof_and_output() {
    let key = VrfKey::from_seed(&[3; 32]);
    let (p1, b1) = vrf_prove(&key, b"window/0");
    let (p2, b2) = vrf_prove(&key, b"window/0");
    assert_eq!(p1.0, p2.0);
    assert_eq!(b1, b2);
}

#[test]
fn different_keys_yield_different_outputs_for_same_alpha() {
    let k1 = VrfKey::from_seed(&[1; 32]);
    let k2 = VrfKey::from_seed(&[2; 32]);
    let (_, b1) = vrf_prove(&k1, b"window/0");
    let (_, b2) = vrf_prove(&k2, b"window/0");
    assert_ne!(b1, b2);
}

#[test]
fn proof_verifies_only_against_own_pubkey() {
    let k1 = VrfKey::from_seed(&[1; 32]);
    let k2 = VrfKey::from_seed(&[2; 32]);
    let (proof, _) = vrf_prove(&k1, b"alpha");
    vrf_verify(&k1.pubkey(), b"alpha", &proof).expect("own key verifies");
    assert!(vrf_verify(&k2.pubkey(), b"alpha", &proof).is_err());
}
