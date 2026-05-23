//! Inactivity leak integration (plan 03d+).

#[test]
fn probe_emits_after_four_unfinalized_windows() {
    let cfg = consensus::Config::default_table_17_1();
    assert!(consensus::macro_fin::probe_inactivity_leak_streak(&cfg));
}

#[test]
fn happy_path_does_not_emit_inactivity_leak() {
    let mut world = sim::world::World::new(4, [1; 32], consensus::Config::default_table_17_1());
    world.run(96);
    assert_eq!(world.inactivity_leak_count(), 0);
}
