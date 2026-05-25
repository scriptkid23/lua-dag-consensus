//! Gossipsub topics. Topic name strings are the wire identifier; never
//! edit them — append new variants instead.

use consensus::event::SubnetId;
use libp2p::gossipsub::IdentTopic;

/// Stable gossipsub topic strings (wire format).
pub mod wire {
    /// Topic prefix shared by all LUA-DAG gossip topics.
    pub const PREFIX: &str = "lua-dag/v1/";

    /// L1 certified vertex stream.
    pub const CERTIFIED_VERTEX: &str = "lua-dag/v1/certified-vertex";
    /// `MicroQC` dissemination.
    pub const MICRO_QC: &str = "lua-dag/v1/micro-qc";
    /// `MacroProposal` dissemination.
    pub const MACRO_PROPOSAL: &str = "lua-dag/v1/macro-proposal";
    /// Per-subnet partial signatures (Mode A).
    pub const BLS_PARTIAL_PREFIX: &str = "lua-dag/v1/bls-partial/";
    /// Subnet aggregate dissemination.
    pub const SUBNET_AGGREGATE: &str = "lua-dag/v1/subnet-aggregate";
    /// Macro QC dissemination.
    pub const MACRO_QC: &str = "lua-dag/v1/macro-qc";
    /// Slashing evidence broadcast.
    pub const SLASH_EVIDENCE: &str = "lua-dag/v1/slash-evidence";
    /// Sequential blob payload chunk stream (L1 07b).
    pub const BLOB_CHUNK: &str = "lua-dag/v1/blob-chunk";
}

/// All gossip topics used by LUA-DAG.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Topic {
    /// L1 certified vertex stream.
    CertifiedVertex,
    /// `MicroQC` dissemination.
    MicroQc,
    /// `MacroProposal` dissemination.
    MacroProposal,
    /// Per-subnet partial signatures (Mode A).
    BlsPartial(SubnetId),
    /// Subnet aggregate dissemination.
    SubnetAggregate,
    /// Macro QC dissemination.
    MacroQc,
    /// Slashing evidence broadcast.
    SlashEvidence,
    /// Sequential blob payload chunk stream (L1 07b).
    BlobChunk,
}

impl Topic {
    /// Stable wire name for this topic.
    #[must_use]
    pub fn wire_name(self) -> String {
        match self {
            Self::CertifiedVertex => wire::CERTIFIED_VERTEX.to_string(),
            Self::MicroQc => wire::MICRO_QC.to_string(),
            Self::MacroProposal => wire::MACRO_PROPOSAL.to_string(),
            Self::BlsPartial(s) => format!("{}{}", wire::BLS_PARTIAL_PREFIX, s.0),
            Self::SubnetAggregate => wire::SUBNET_AGGREGATE.to_string(),
            Self::MacroQc => wire::MACRO_QC.to_string(),
            Self::SlashEvidence => wire::SLASH_EVIDENCE.to_string(),
            Self::BlobChunk => wire::BLOB_CHUNK.to_string(),
        }
    }

    /// Parse a gossipsub topic string into a [`Topic`], if recognized.
    #[must_use]
    pub fn from_wire_name(s: &str) -> Option<Self> {
        match s {
            wire::CERTIFIED_VERTEX => Some(Self::CertifiedVertex),
            wire::MICRO_QC => Some(Self::MicroQc),
            wire::MACRO_PROPOSAL => Some(Self::MacroProposal),
            wire::SUBNET_AGGREGATE => Some(Self::SubnetAggregate),
            wire::MACRO_QC => Some(Self::MacroQc),
            wire::SLASH_EVIDENCE => Some(Self::SlashEvidence),
            wire::BLOB_CHUNK => Some(Self::BlobChunk),
            s if s.starts_with(wire::BLS_PARTIAL_PREFIX) => {
                let rest = s.strip_prefix(wire::BLS_PARTIAL_PREFIX)?;
                let id = rest.parse().ok()?;
                Some(Self::BlsPartial(SubnetId(id)))
            }
            _ => None,
        }
    }

    /// Stable wire name as a gossipsub [`IdentTopic`].
    #[must_use]
    pub fn ident(self) -> IdentTopic {
        IdentTopic::new(self.wire_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_topic_includes_subnet_id() {
        let t = Topic::BlsPartial(SubnetId(7)).ident();
        assert!(t.to_string().ends_with("/7"));
    }

    #[test]
    fn topics_are_distinct_strings() {
        let names = [
            Topic::CertifiedVertex,
            Topic::MicroQc,
            Topic::MacroProposal,
            Topic::SubnetAggregate,
            Topic::MacroQc,
            Topic::SlashEvidence,
            Topic::BlobChunk,
        ]
        .map(|t| t.ident().to_string());
        let mut sorted = names.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }

    #[test]
    fn from_wire_name_round_trips_fixed_topics() {
        for topic in [
            Topic::CertifiedVertex,
            Topic::MicroQc,
            Topic::MacroProposal,
            Topic::SubnetAggregate,
            Topic::MacroQc,
            Topic::SlashEvidence,
            Topic::BlobChunk,
        ] {
            let name = topic.wire_name();
            assert_eq!(Topic::from_wire_name(&name), Some(topic), "{name}");
        }
    }

    #[test]
    fn from_wire_name_parses_bls_partial_subnet() {
        let name = Topic::BlsPartial(SubnetId(42)).wire_name();
        assert_eq!(
            Topic::from_wire_name(&name),
            Some(Topic::BlsPartial(SubnetId(42)))
        );
    }
}
