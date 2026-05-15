//! Decode + dump artifacts from a `RocksDB` directory.

use anyhow::Result;
use rocksdb::IteratorMode;
use serde::Serialize;
use storage::{Database, StorageConfig, columns::ColumnFamily};

use crate::args::{InspectArgs, InspectKind};

#[derive(Serialize)]
struct Summary {
    column: &'static str,
}

/// Entrypoint.
pub fn run(args: &InspectArgs) -> Result<()> {
    let cfg = StorageConfig {
        path: args.db.clone(),
        create_if_missing: false,
        max_total_wal_size_mb: 64,
    };
    let db = Database::open(&cfg)?;

    match args.kind {
        InspectKind::Summary => {
            let summaries: Vec<Summary> = ColumnFamily::all()
                .iter()
                .map(|cf| Summary { column: cf.name() })
                .collect();
            println!("{}", serde_json::to_string_pretty(&summaries)?);
        }
        InspectKind::MacroCheckpoints => {
            let h = db.cf(ColumnFamily::MacroCheckpoint)?;
            let mut count = 0u64;
            for item in db.raw().iterator_cf(h, IteratorMode::Start) {
                let (_k, v) = item?;
                let cp: types::macros::MacroCheckpoint = borsh::from_slice(&v)
                    .map_err(|e| anyhow::anyhow!("decode MacroCheckpoint: {e}"))?;
                println!("height={:?} hash={}", cp.height, hex::encode(cp.hash.0));
                count += 1;
            }
            eprintln!("dumped {count} macro checkpoints");
        }
        InspectKind::MacroQcs => {
            let h = db.cf(ColumnFamily::MacroQc)?;
            let mut count = 0u64;
            for item in db.raw().iterator_cf(h, IteratorMode::Start) {
                let (_k, v) = item?;
                let qc: types::macros::MacroQc =
                    borsh::from_slice(&v).map_err(|e| anyhow::anyhow!("decode MacroQc: {e}"))?;
                println!(
                    "checkpoint={} mode={:?} signers={}",
                    hex::encode(qc.checkpoint_hash.0),
                    qc.mode,
                    qc.agg
                        .bitmap
                        .iter()
                        .map(|b| b.count_ones() as usize)
                        .sum::<usize>()
                );
                count += 1;
            }
            eprintln!("dumped {count} macro QCs");
        }
    }
    Ok(())
}
