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
use types::macros::{MacroProposal, MacroQc};
use types::micro::MicroQc;
use types::slashing::SlashEvidence;

use crate::error::Result;
use crate::gossip::Topic;
use crate::gossip::codec::{decode_event_payload, encode_action_payload};

const TOPIC_CERTIFIED_VERTEX: &str = "lua-dag/v1/certified-vertex";
const TOPIC_MICRO_QC: &str = "lua-dag/v1/micro-qc";
const TOPIC_MACRO_PROPOSAL: &str = "lua-dag/v1/macro-proposal";
const TOPIC_SUBNET_AGGREGATE: &str = "lua-dag/v1/subnet-aggregate";
const TOPIC_MACRO_QC: &str = "lua-dag/v1/macro-qc";
const TOPIC_SLASH_EVIDENCE: &str = "lua-dag/v1/slash-evidence";
const TOPIC_BLS_PARTIAL_PREFIX: &str = "lua-dag/v1/bls-partial/";

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
        | Action::ScheduleTimer { .. }
        | Action::CancelTimer(_)
        | Action::UpdateBlobStatus { .. } => return Ok(None),
    };
    Ok(Some(pair))
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
    if topic_str == TOPIC_MICRO_QC {
        let m: MicroQc = decode_event_payload(data)?;
        // No dedicated `MicroQcReceived` exists today; surface as Assembled.
        // (Pending alignment with consensus; see plan §10 for owner.)
        return Ok(Some(Event::MicroQcAssembled(m)));
    }
    if topic_str == TOPIC_MACRO_PROPOSAL {
        let m: MacroProposal = decode_event_payload(data)?;
        return Ok(Some(Event::MacroProposalReceived(m)));
    }
    if topic_str == TOPIC_SUBNET_AGGREGATE {
        let a: SubnetAggregate = decode_event_payload(data)?;
        return Ok(Some(Event::SubnetAggregateReceived(a)));
    }
    if topic_str == TOPIC_MACRO_QC {
        let q: MacroQc = decode_event_payload(data)?;
        return Ok(Some(Event::MacroQcReceived(q)));
    }
    if topic_str == TOPIC_SLASH_EVIDENCE {
        let s: SlashEvidence = decode_event_payload(data)?;
        return Ok(Some(Event::SlashEvidenceFound(s)));
    }
    if let Some(rest) = topic_str.strip_prefix(TOPIC_BLS_PARTIAL_PREFIX) {
        // Validate the subnet-id suffix is well-formed even though the
        // authoritative SubnetId is preserved inside the payload itself.
        let _subnet: u32 = rest.parse().map_err(|e: std::num::ParseIntError| {
            crate::error::Error::Codec(format!("bad subnet id `{rest}`: {e}"))
        })?;
        let p: BlsPartial = decode_event_payload(data)?;
        return Ok(Some(Event::BlsPartialReceived(p)));
    }
    if topic_str == TOPIC_CERTIFIED_VERTEX {
        // Mode-A devnet does not produce CertifiedVertex broadcasts; subscribers
        // ignore until L1 ingestion lands.
        return Ok(None);
    }
    Ok(None)
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
    fn timer_action_is_not_broadcast() {
        let action = Action::CancelTimer(TimerId(1));
        assert!(outbound_broadcast(&action).unwrap().is_none());
        assert!(!is_broadcast(&action));
    }

    #[test]
    fn certified_vertex_topic_returns_none() {
        // Subscribed but no Event mapping yet: must not error.
        let ev = inbound_message(TOPIC_CERTIFIED_VERTEX, &[]).unwrap();
        assert!(ev.is_none());
    }

    #[test]
    fn unknown_topic_returns_none() {
        let ev = inbound_message("lua-dag/v1/unknown", &[]).unwrap();
        assert!(ev.is_none());
    }
}
