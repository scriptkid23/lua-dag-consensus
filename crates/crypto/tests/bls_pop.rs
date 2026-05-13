//! Proof-of-Possession across rotated keys.

use crypto::bls::{SecretKey, generate_pop, verify_pop};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

#[test]
fn cross_validator_pop_isolation() {
    let mut rng = ChaCha20Rng::from_seed([42; 32]);
    let sk_a = SecretKey::random(&mut rng).unwrap();
    let sk_b = SecretKey::random(&mut rng).unwrap();
    let pop_a = generate_pop(&sk_a);
    // A's PoP must verify under A's key but not under B's key.
    verify_pop(&sk_a.public(), &pop_a).expect("PoP must verify against owner");
    let err = verify_pop(&sk_b.public(), &pop_a).unwrap_err();
    assert!(matches!(err, crypto::Error::PopInvalid));
}
