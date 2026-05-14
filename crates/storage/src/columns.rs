//! Column-family names and helpers.
//!
//! Each variant maps to a single column family; the wire name (the `&str`
//! returned by [`ColumnFamily::name`]) is part of the on-disk format and
//! must not change without a migration.

/// All column families in the LUA-DAG store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ColumnFamily {
    /// `(round, author) -> CertifiedVertex`.
    Vertex,
    /// `slot -> MicroCheckpoint`.
    MicroCheckpoint,
    /// `slot -> MicroQc`.
    MicroQc,
    /// `height -> MacroCheckpoint`.
    MacroCheckpoint,
    /// `checkpoint_hash -> MacroQc`.
    MacroQc,
    /// `height -> 2-chain pointer (parent_hash)`.
    MacroTwoChain,
    /// `epoch -> ValidatorSet`.
    ValidatorSet,
    /// `seq -> SlashEvidence` (append-only).
    SlashEvidence,
    /// `(validator, target_epoch) -> VoteRecord`.
    VoteBook,
}

impl ColumnFamily {
    /// Wire name (on-disk).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::MicroCheckpoint => "micro_cp",
            Self::MicroQc => "micro_qc",
            Self::MacroCheckpoint => "macro_cp",
            Self::MacroQc => "macro_qc",
            Self::MacroTwoChain => "macro_two_chain",
            Self::ValidatorSet => "valset",
            Self::SlashEvidence => "slash",
            Self::VoteBook => "votebook",
        }
    }

    /// Complete list (used at DB-open time).
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Vertex,
            Self::MicroCheckpoint,
            Self::MicroQc,
            Self::MacroCheckpoint,
            Self::MacroQc,
            Self::MacroTwoChain,
            Self::ValidatorSet,
            Self::SlashEvidence,
            Self::VoteBook,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_distinct() {
        let mut names: Vec<_> = ColumnFamily::all().iter().map(|c| c.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ColumnFamily::all().len());
    }
}
