//! Smoke test for the GC plan helper. Real prune-and-delete lives in a
//! follow-up plan.

use consensus::Config;
use storage::gc;

#[test]
fn plan_uses_configured_horizons() {
    let cfg = Config::default_table_17_1();
    let plan = gc::plan(&cfg, 5_000);
    assert_eq!(
        plan.hot_horizon_round,
        5_000 - cfg.storage.gc_hot_horizon_rounds
    );
}
