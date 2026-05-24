//! Blob chunk encode/decode roundtrip on gossip wire (07b).

use dag::blob::chunk::split_payload;
use net::gossip::Topic;
use net::gossip_wire::{decode_blob_chunk, encode_blob_chunk};
use types::primitives::BlobId;

#[test]
fn blob_chunk_encode_decode_roundtrip() {
    let payload = vec![0xEFu8; 70_000];
    let chunk = split_payload(&payload, 65_536).into_iter().next().unwrap();
    let (topic, bytes) = encode_blob_chunk(&chunk).unwrap();
    assert_eq!(topic, Topic::BlobChunk);
    let decoded = decode_blob_chunk(&topic.wire_name(), &bytes)
        .unwrap()
        .expect("blob chunk");
    assert_eq!(decoded, chunk);
}

#[test]
fn decode_returns_none_for_other_topics() {
    let got = decode_blob_chunk(Topic::MicroQc.wire_name().as_str(), &[]).unwrap();
    assert!(got.is_none());
}

#[test]
fn chunk_carries_blob_id_and_index() {
    let payload = b"rollup-batch-v0";
    let chunks = split_payload(payload, 32);
    assert_eq!(chunks.len(), 1);
    assert_ne!(chunks[0].blob_id, BlobId([0; 32]));
    assert_eq!(chunks[0].index(), 0);
}
