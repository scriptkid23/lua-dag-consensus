//! Dev-only binary: proves `RocksDB` + `storage::Database::open` in Linux containers.

use std::{path::PathBuf, thread};

fn main() {
    let path = env_path();
    let cfg = storage::StorageConfig {
        path,
        create_if_missing: true,
        max_total_wal_size_mb: 256,
    };

    match storage::Database::open(&cfg) {
        Ok(_) => {
            eprintln!(
                "lua-dag-smoke: opened RocksDB at {}",
                cfg.path.display()
            );
        }
        Err(e) => {
            eprintln!("lua-dag-smoke: FATAL {e:?}");
            std::process::exit(1);
        }
    }

    // Stay alive for Docker Compose smoke (SIGTERM kills the process).
    loop {
        thread::park();
    }
}

fn env_path() -> PathBuf {
    let key = "STORAGE_PATH";
    match std::env::var_os(key) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from("/data/rocksdb"),
    }
}
