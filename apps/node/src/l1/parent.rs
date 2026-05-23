//! Parent-hash selection for L1 vertex chains (mirrors `sim::World::parent_hash_for_round`).

use consensus::{
    bullshark::{select_anchor, WaveId},
    config::Config,
    error::Result,
    ports::{DagView, RandomnessBeacon},
};
use types::{
    crypto_types::Hash32,
    primitives::Round,
    validator::ValidatorSet,
};

fn anchor_hash_for_wave(
    wave: WaveId,
    dag: &dyn DagView,
    cfg: &Config,
    valset: &ValidatorSet,
    beacon: &dyn RandomnessBeacon,
) -> Result<Option<Hash32>> {
    let choice = select_anchor(wave, valset, beacon, &cfg.leader)?;
    let anchor_round = wave.first_round();
    Ok(dag
        .vertices_at_round(anchor_round)?
        .into_iter()
        .find(|v| v.vertex.author == choice.author)
        .map(|v| v.vertex.hash))
}

/// Parent link for round `round`, matching sim Bullshark DAG semantics.
pub fn parent_hash_for_round(
    round: u64,
    dag: &dyn DagView,
    cfg: &Config,
    valset: &ValidatorSet,
    beacon: &dyn RandomnessBeacon,
) -> Result<Option<Hash32>> {
    if round == 0 {
        return Ok(None);
    }
    let wave = WaveId::of_round(Round(round));
    let anchor_round = wave.first_round().0;
    if round > anchor_round {
        if let Some(h) = anchor_hash_for_wave(wave, dag, cfg, valset, beacon)? {
            return Ok(Some(h));
        }
    }
    let prev = Round(round - 1);
    let mut verts = dag.vertices_at_round(prev)?;
    verts.sort_by_key(|v| v.vertex.hash.0);
    Ok(verts.first().map(|v| v.vertex.hash))
}
