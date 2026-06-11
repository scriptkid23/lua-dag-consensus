//! L1 distributed-certification wire messages (2026-06-04 design).
//!
//! `Serialize`/`Deserialize` are not derived because `BlsSig` is
//! wire-only (Borsh) — same convention as [`super::CertifiedVertex`].

use borsh::{BorshDeserialize, BorshSerialize};

use super::vertex::Vertex;
use crate::{
    crypto_types::{BlsSig, Hash32},
    primitives::{Round, ValidatorId},
};

/// Header a node proposes for its own round (not yet certified).
///
/// `proposer_sig` signs `dag::signing::signing_bytes(vertex)` under
/// DST `lua-dag/v1/vertex-proposal` (propose authority — distinct from
/// the partial vote so a vote can never be replayed as a proposal).
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexProposal {
    /// The proposed vertex (`author = proposer`, sealed `hash`).
    pub vertex: Vertex,
    /// Proposer authority signature (DST `VERTEX_PROPOSAL`).
    pub proposer_sig: BlsSig,
}

/// A single validator's partial vote on a proposal.
///
/// `sig` signs `dag::signing::signing_bytes(vertex)` under DST
/// `lua-dag/v1/vertex-cert` — exactly the message
/// `dag::cert::verify_certified_vertex` checks, so aggregation is just
/// signature collection + bitmap.
#[derive(Clone, Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct VertexPartial {
    /// Hash of the voted vertex.
    pub vertex_hash: Hash32,
    /// Round of the voted vertex (routing / memory bound).
    pub round: Round,
    /// Proposal owner — only this validator aggregates the partials.
    pub author: ValidatorId,
    /// The validator signing this partial.
    pub voter: ValidatorId,
    /// BLS partial signature (DST `VERTEX_CERT`).
    pub sig: BlsSig,
}
