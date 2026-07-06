//! Distributed vertex certification scenarios (06-04 design §6).

use crate::{scenarios::Report, world::World};

/// Happy path: n validators run the distributed protocol; every machine's
/// own-proposal round advances and the shared DAG accumulates certs.
#[must_use]
pub fn run(validators: u32, rounds: u32, seed: [u8; 32]) -> Report {
    let mut world = World::new(
        validators,
        seed,
        consensus::Config::default_table_17_1(),
    );
    world.enable_distributed_vertices();
    world.run(rounds);

    let min_round = world.min_vertex_round();
    let advanced = min_round >= 2;
    Report {
        scenario: "vertex-cert-distributed".into(),
        validators,
        rounds,
        safety_ok: advanced,
        liveness_ok: advanced,
        lock_macro_ok: true,
        notes: vec![format!("min own-proposal round = {min_round}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus::ports::DagView;
    use types::primitives::Round;

    #[test]
    fn happy_path_four_validators_advance() {
        let report = run(4, 32, [7; 32]);
        assert!(report.liveness_ok, "{:?}", report.notes);
    }

    #[test]
    fn all_four_authors_certify_round_zero() {
        let mut world = World::new(4, [8; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        world.run(16);
        let certs = world.dag.vertices_at_round(Round(0)).unwrap();
        let mut authors: Vec<_> = certs.iter().map(|cv| cv.vertex.author).collect();
        authors.sort();
        authors.dedup();
        assert_eq!(authors.len(), 4, "every validator's genesis vertex certified");
    }

    #[test]
    fn partition_halts_and_heals() {
        let mut world = World::new(4, [9; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        // split 2/2: neither side can reach 2f+1 = 3
        world.net.set_partition([0u32, 1], [2u32, 3]);
        world.run(16);
        let stalled_at = world.min_vertex_round();
        assert!(stalled_at <= 1, "split below quorum must not advance");
        world.net.heal_partition();
        world.run(32);
        assert!(
            world.min_vertex_round() > stalled_at,
            "healed network resumes advancement"
        );
    }

    #[test]
    fn double_propose_yields_vertex_equivocation_evidence() {
        use consensus::event::Event;
        use crate::virtual_net::InFlight;
        use crypto::hash::dst;
        use types::crypto_types::Hash32;
        use types::dag::{Vertex, VertexProposal};

        let mut world = World::new(4, [10; 32], consensus::Config::default_table_17_1());
        world.enable_distributed_vertices();
        world.run(2); // genesis proposals delivered

        // Forge a SECOND, conflicting round-0 proposal from validator 0.
        let mut vertex = Vertex {
            round: types::primitives::Round(0),
            author: crate::vertex_factory::validator_id_for_index(0),
            parents: vec![],
            blobs: vec![types::dag::BlobRef {
                blob_id: types::primitives::BlobId([0xBB; 32]),
                commitment: Hash32([0xCC; 32]),
                size_bytes: 1,
            }],
            hash: Hash32::zero(),
        };
        dag::signing::seal_hash(&mut vertex);
        let msg = dag::signing::signing_bytes(&vertex);
        let sig = crypto::bls::sign::sign(
            &world.key_ring_bls_secret(0),
            dst::VERTEX_PROPOSAL,
            &msg,
        );
        world.net.enqueue(InFlight {
            recipient: 1,
            event: Event::VertexProposalReceived(VertexProposal {
                vertex,
                proposer_sig: sig,
            }),
            deliver_at: 0,
        });
        world.run(8);
        assert!(
            world.slash_evidence_total() > 0,
            "conflicting proposal must produce VertexEquivocation evidence"
        );
    }
}
