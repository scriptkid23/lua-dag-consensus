# Protocol Parameters

`default.toml` mirrors whitepaper Table 17.1.

`node`, `sim`, and `cli` all load this file as their base configuration; any
override file (e.g. `config/local.toml`, gitignored) is merged on top.

When changing a parameter:
1. Update `default.toml` here.
2. Update `crates/consensus/src/config.rs` so the typed constant matches.
3. Note the change in `docs/architecture/` if it has cross-component effects.

Loaders **must reject** unknown `schema_version` values — bump the version
when the schema (not just values) changes.
