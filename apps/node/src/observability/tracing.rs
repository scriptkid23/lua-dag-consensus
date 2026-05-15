//! `tracing-subscriber` initialisation.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialise structured (JSON) tracing with `RUST_LOG`-driven filter.
///
/// Idempotent under repeated calls; subsequent calls become no-ops.
pub fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,libp2p=warn,rocksdb=warn"));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json().with_target(true))
            .try_init();
    });
}
