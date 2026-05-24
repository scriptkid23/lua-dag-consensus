//! Bridges consensus `Action` ↔ gossip topics + Borsh payloads.
//!
//! Two total functions:
//!   * [`outbound_broadcast`] — consensus `Action` → `(Topic, payload)`
//!   * [`inbound_message`]    — `(topic_str, payload)` → consensus `Event`
//!
//! Both return `Ok(None)` only for variants that intentionally have no wire
//! counterpart — never silently for serialization failures.

use consensus::action::Action;
use consensus::event::{BlsPartial, Event, SubnetAggregate};
use dag::blob::chunk::BlobChunk;
use types::dag::CertifiedVertex;
use types::macros::{MacroProposal, MacroQc};
use types::micro::MicroQc;
use types::slashing::SlashEvidence;

use crate::error::Result;
use crate::gossip::Topic;
use crate::gossip::codec::{decode_event_payload, encode_action_payload};

/// Map a consensus `Action` to its gossip topic + Borsh payload.
///
/// Returns `Ok(None)` for actions that are intentionally host-local
/// (timers, persistence, blob status). Returns `Err` if encoding fails —
/// never drop a broadcast silently.
pub fn outbound_broadcast(action: &Action) -> Result<Option<(Topic, Vec<u8>)>> {
    let pair = match action {
        Action::BroadcastMicroQc(m) => (Topic::MicroQc, encode_action_payload(m)?),
        Action::BroadcastMacroProposal(m) => (Topic::MacroProposal, encode_action_payload(m)?),
        Action::BroadcastBlsPartial(p) => (Topic::BlsPartial(p.subnet), encode_action_payload(p)?),
        Action::BroadcastSubnetAggregate(a) => (Topic::SubnetAggregate, encode_action_payload(a)?),
        Action::BroadcastMacroQc(q) => (Topic::MacroQc, encode_action_payload(q)?),
        Action::EmitSlashEvidence { evidence, .. } => {
            (Topic::SlashEvidence, encode_action_payload(evidence)?)
        }
        Action::PersistMacroQc(_)
        | Action::PersistMacroCheckpoint(_)
        | Action::ScheduleTimer { .. }
        | Action::CancelTimer(_)
        | Action::UpdateBlobStatus { .. }
        | Action::NotifyInactivityLeak { .. } => return Ok(None),
    };
    Ok(Some(pair))
}

/// Encode a certified vertex for gossip publish (host L1 driver path).
pub fn encode_certified_vertex(cv: &CertifiedVertex) -> Result<(Topic, Vec<u8>)> {
    Ok((Topic::CertifiedVertex, encode_action_payload(cv)?))
}

/// Encode a blob chunk for gossip publish (host blob custody path).
pub fn encode_blob_chunk(chunk: &BlobChunk) -> Result<(Topic, Vec<u8>)> {
    Ok((
        Topic::BlobChunk,
        borsh::to_vec(chunk).map_err(|e| crate::error::Error::Codec(e.to_string()))?,
    ))
}

/// Decode an inbound blob chunk; returns `None` when `topic` is not blob-chunk.
pub fn decode_blob_chunk(topic: &str, data: &[u8]) -> Result<Option<BlobChunk>> {
    if Topic::from_wire_name(topic) != Some(Topic::BlobChunk) {
        return Ok(None);
    }
    let chunk = borsh::from_slice(data).map_err(|e| crate::error::Error::Codec(e.to_string()))?;
    Ok(Some(chunk))
}

/// Returns `true` iff this action would have been published by [`outbound_broadcast`].
///
/// Cheap pre-flight used by the orchestrator to route broadcast actions onto
/// the gossip channel and keep timer/persistence actions on the local path.
#[must_use]
pub fn is_broadcast(action: &Action) -> bool {
    matches!(
        action,
        Action::BroadcastMicroQc(_)
            | Action::BroadcastMacroProposal(_)
            | Action::BroadcastBlsPartial(_)
            | Action::BroadcastSubnetAggregate(_)
            | Action::BroadcastMacroQc(_)
            | Action::EmitSlashEvidence { .. }
    )
}

/// Map an inbound gossipsub message to a consensus `Event`.
///
/// Returns `Ok(None)` for topics we subscribe to but do not yet have an
/// `Event` mapping for (e.g. `CertifiedVertex` if upstream feed is added
/// later). Returns `Err` on decode failure — callers may log and continue
/// rather than terminate the swarm.
pub fn inbound_message(topic_str: &str, data: &[u8]) -> Result<Option<Event>> {
    let Some(topic) = Topic::from_wire_name(topic_str) else {
        return Ok(None);
    };
    match topic {
        Topic::MicroQc => {
            let m: MicroQc = decode_event_payload(data)?;
            // No dedicated `MicroQcReceived` exists today; surface as Assembled.
            Ok(Some(Event::MicroQcAssembled(m)))
        }
        Topic::MacroProposal => {
            let m: MacroProposal = decode_event_payload(data)?;
            Ok(Some(Event::MacroProposalReceived(m)))
        }
        Topic::SubnetAggregate => {
            let a: SubnetAggregate = decode_event_payload(data)?;
            Ok(Some(Event::SubnetAggregateReceived(a)))
        }
        Topic::MacroQc => {
            let q: MacroQc = decode_event_payload(data)?;
            Ok(Some(Event::MacroQcReceived(q)))
        }
        Topic::SlashEvidence => {
            let s: SlashEvidence = decode_event_payload(data)?;
            Ok(Some(Event::SlashEvidenceFound(s)))
        }
        Topic::BlsPartial(subnet) => {
            let p: BlsPartial = decode_event_payload(data)?;
            if p.subnet != subnet {
                return Err(crate::error::Error::Codec(format!(
                    "bls-partial topic subnet {} != payload subnet {}",
                    subnet.0, p.subnet.0
                )));
            }
            Ok(Some(Event::BlsPartialReceived(p)))
        }
        Topic::CertifiedVertex => {
            let v: CertifiedVertex = decode_event_payload(data)?;
            Ok(Some(Event::CertifiedVertexReceived(v)))
        }
        Topic::BlobChunk => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus::action::Action;
    use consensus::event::{BlsPartial, SubnetId, TimerId};
    use types::crypto_types::{BlsAggSig, BlsSig, Hash32};
    use types::micro::MicroQc;
    use types::primitives::ValidatorId;

    fn micro_qc_fixture() -> MicroQc {
        MicroQc {
            checkpoint_hash: Hash32([0u8; 32]),
            agg: BlsAggSig {
                sig: BlsSig([0u8; 96]),
                bitmap: vec![0xFF],
            },
        }
    }

    fn bls_partial_fixture(subnet: u32) -> BlsPartial {
        BlsPartial {
            subnet: SubnetId(subnet),
            validator: ValidatorId([1u8; 32]),
            checkpoint_hash: Hash32([2u8; 32]),
            sig: BlsSig([3u8; 96]),
        }
    }

    #[test]
    fn micro_qc_round_trip() {
        let m = micro_qc_fixture();
        let action = Action::BroadcastMicroQc(m.clone());
        let (topic, payload) = outbound_broadcast(&action).unwrap().unwrap();
        let topic_str = topic.ident().to_string();
        let ev = inbound_message(&topic_str, &payload).unwrap().unwrap();
        assert!(matches!(ev, Event::MicroQcAssembled(_)));
    }

    #[test]
    fn bls_partial_round_trip_preserves_subnet() {
        let p = bls_partial_fixture(7);
        let action = Action::BroadcastBlsPartial(p.clone());
        let (topic, payload) = outbound_broadcast(&action).unwrap().unwrap();
        let topic_str = topic.ident().to_string();
        assert!(topic_str.ends_with("/7"));
        let ev = inbound_message(&topic_str, &payload).unwrap().unwrap();
        match ev {
            Event::BlsPartialReceived(p2) => assert_eq!(p, p2),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn certified_vertex_roundtrips_on_wire() {
        use types::dag::{CertifiedVertex, Vertex};
        let v = CertifiedVertex {
            vertex: Vertex {
                round: types::primitives::Round(5),
                author: ValidatorId([6; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([6; 32]),
            },
            certificate: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        let bytes = crate::gossip::codec::encode_action_payload(&v).unwrap();
        let topic = Topic::CertifiedVertex;
        let ev = inbound_message(&topic.ident().to_string(), &bytes)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Event::CertifiedVertexReceived(got) if got == v));
    }

    #[test]
    fn certified_vertex_encode_for_publish() {
        use types::dag::{CertifiedVertex, Vertex};
        let v = CertifiedVertex {
            vertex: Vertex {
                round: types::primitives::Round(5),
                author: ValidatorId([6; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([6; 32]),
            },
            certificate: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        let (topic, bytes) = encode_certified_vertex(&v).unwrap();
        assert_eq!(topic, Topic::CertifiedVertex);
        let ev = inbound_message(&topic.ident().to_string(), &bytes)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Event::CertifiedVertexReceived(got) if got == v));
    }

    #[test]
    fn timer_action_is_not_broadcast() {
        let action = Action::CancelTimer(TimerId(1));
        assert!(outbound_broadcast(&action).unwrap().is_none());
        assert!(!is_broadcast(&action));
    }

    #[test]
    fn certified_vertex_topic_decodes_to_event() {
        use types::dag::{CertifiedVertex, Vertex};
        let v = CertifiedVertex {
            vertex: Vertex {
                round: types::primitives::Round(1),
                author: types::primitives::ValidatorId([1; 32]),
                parents: vec![],
                blobs: vec![],
                hash: Hash32([1; 32]),
            },
            certificate: BlsAggSig {
                sig: BlsSig([0; 96]),
                bitmap: vec![0xFF],
            },
        };
        let bytes = crate::gossip::codec::encode_action_payload(&v).unwrap();
        let ev = inbound_message(&Topic::CertifiedVertex.wire_name(), &bytes).unwrap();
        assert!(matches!(ev, Some(Event::CertifiedVertexReceived(_))));
    }

    #[test]
    fn unknown_topic_returns_none() {
        let ev = inbound_message("lua-dag/v1/unknown", &[]).unwrap();
        assert!(ev.is_none());
    }

    #[test]
    fn blob_chunk_encode_decode_roundtrip() {
        use dag::blob::chunk::split_payload;
        let payload = vec![0xEFu8; 70_000];
        let chunk = split_payload(&payload, 65_536).into_iter().next().unwrap();
        let (topic, bytes) = encode_blob_chunk(&chunk).unwrap();
        assert_eq!(topic, Topic::BlobChunk);
        let decoded = decode_blob_chunk(&topic.wire_name(), &bytes)
            .unwrap()
            .expect("chunk");
        assert_eq!(decoded, chunk);
    }
}
