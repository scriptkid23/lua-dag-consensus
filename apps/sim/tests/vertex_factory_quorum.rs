//! Multi-vertex factory: each round carries a `2f+1` quorum.

use consensus::{
    Config,
    bullshark::WaveId,
    ports::{DagView, ValidatorSetPort},
};
use sim::{vertex_factory::quorum_vertex_count, world::World};
use types::primitives::Round;

#[test]
fn one_wave_has_quorum_vertices_per_round() {
    let n = 4;
    let quorum = quorum_vertex_count(n);
    let mut world = World::new(n, [0xAB; 32], Config::default_table_17_1());
    world.run(4);

    for r in 0..4 {
        let verts = world.dag.vertices_at_round(Round(r)).expect("dag query");
        assert_eq!(
            verts.len(),
            usize::try_from(quorum).expect("quorum fits usize"),
            "round {r} must carry 2f+1 certified vertices"
        );
    }
}

#[test]
fn wave0_meets_shortcut_support_after_one_wave() {
    use consensus::bullshark::select_anchor;
    use consensus::ports::DagView;

    let n = 4;
    let cfg = Config::default_table_17_1();
    let mut world = World::new(n, [0xCD; 32], cfg.clone());
    // One full wave (rounds 0–3) plus shortcut window rounds 4–5.
    let ticks = 4 + u64::from(cfg.bullshark.shortcut_round_count) + 1;
    world.run(u32::try_from(ticks).expect("tick count fits u32"));

    let set = world
        .valset
        .set_for(types::primitives::Epoch(0))
        .expect("valset")
        .expect("epoch 0 set");
    let anchor = select_anchor(WaveId(0), &set, world.beacon.as_ref(), &cfg.leader)
        .expect("anchor selection");
    let anchor_hash = world
        .dag
        .vertices_at_round(WaveId(0).first_round())
        .expect("dag")
        .into_iter()
        .find(|v| v.vertex.author == anchor.author)
        .expect("anchor vertex in dag")
        .vertex
        .hash;

    let mut supporters = 0u32;
    for offset in 1..=u64::from(cfg.bullshark.shortcut_round_count) {
        let round = Round(offset);
        for v in world.dag.vertices_at_round(round).expect("dag") {
            if v.vertex.parents.iter().any(|p| p == &anchor_hash) {
                supporters += 1;
            }
        }
    }
    assert!(
        supporters >= quorum_vertex_count(n),
        "shortcut window must include at least 2f+1 vertices parented to anchor, got {supporters}"
    );
}
