//! Gossipsub topics. Topic name strings are the wire identifier; never
//! edit them — append new variants instead.

use consensus::event::SubnetId;
use libp2p::gossipsub::IdentTopic;

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
}

impl Topic {
    /// Stable wire name.
    #[must_use]
    pub fn ident(self) -> IdentTopic {
        let name = match self {
            Self::CertifiedVertex => "lua-dag/v1/certified-vertex".to_string(),
            Self::MicroQc => "lua-dag/v1/micro-qc".to_string(),
            Self::MacroProposal => "lua-dag/v1/macro-proposal".to_string(),
            Self::BlsPartial(s) => format!("lua-dag/v1/bls-partial/{}", s.0),
            Self::SubnetAggregate => "lua-dag/v1/subnet-aggregate".to_string(),
            Self::MacroQc => "lua-dag/v1/macro-qc".to_string(),
            Self::SlashEvidence => "lua-dag/v1/slash-evidence".to_string(),
        };
        IdentTopic::new(name)
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
        ]
        .map(|t| t.ident().to_string());
        let mut sorted = names.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
    }
}
