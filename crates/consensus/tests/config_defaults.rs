//! Confirm the in-crate defaults match `config/default.toml`.

use consensus::Config;

#[test]
fn in_crate_defaults_parse_round_trip_via_toml() {
    let cfg = Config::default_table_17_1();
    let s = toml::to_string(&cfg).expect("serialize");
    let parsed = Config::from_toml_str(&s).expect("parse");
    assert_eq!(cfg, parsed);
}

#[test]
fn default_toml_file_matches_in_crate_defaults() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/default.toml"
    ))
    .expect("read config/default.toml from workspace root");
    let parsed = Config::from_toml_str(&raw).expect("parse default.toml");
    let in_crate = Config::default_table_17_1();
    assert_eq!(
        parsed, in_crate,
        "config/default.toml drifted from consensus::Config::default_table_17_1()"
    );
}
