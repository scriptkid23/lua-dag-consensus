use dag::erasure::{decode_shards, encode_shards, rs_merkle_commitment, ErasureConfig};

#[test]
fn recovers_payload_from_k_of_n_shards() {
    let cfg = ErasureConfig {
        k: 4,
        n: 6,
        data_shard_size: 32 * 1024,
    };
    let payload = vec![0xCDu8; 100_000];
    let shards = encode_shards(&payload, &cfg).unwrap();
    assert_eq!(shards.len(), 6);
    let subset: Vec<_> = shards
        .into_iter()
        .enumerate()
        .take(4)
        .map(|(i, data)| (u32::try_from(i).unwrap(), data))
        .collect();
    let recovered = decode_shards(&subset, &cfg, payload.len()).unwrap();
    assert_eq!(recovered, payload);
}

#[test]
fn rs_commitment_changes_when_shard_tampered() {
    let cfg = ErasureConfig::devnet_default();
    let payload = b"batch";
    let shards = encode_shards(payload, &cfg).unwrap();
    let c0 = dag::erasure::rs_merkle_commitment(&shards);
    let mut bad = shards.clone();
    bad[0][0] ^= 0x01;
    let c1 = dag::erasure::rs_merkle_commitment(&bad);
    assert_ne!(c0, c1);
}
