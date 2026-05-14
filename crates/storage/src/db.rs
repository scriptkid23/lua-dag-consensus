//! `RocksDB` handle wrapper.

use rocksdb::{ColumnFamilyDescriptor, DB, Options};

use crate::{
    columns::ColumnFamily,
    config::StorageConfig,
    error::{Error, Result},
};

/// Opened `RocksDB` handle.
#[derive(Debug)]
pub struct Database {
    /// The underlying `rocksdb::DB`. We keep it `pub(crate)` so stores
    /// can borrow it directly without exposing rocksdb in the crate API.
    pub(crate) inner: DB,
}

impl Database {
    /// Open (creating if missing) a `RocksDB` instance with all column
    /// families from [`ColumnFamily::all`].
    pub fn open(cfg: &StorageConfig) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(cfg.create_if_missing);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_total_wal_size(cfg.max_total_wal_size_mb * 1024 * 1024);

        let cf_descriptors: Vec<ColumnFamilyDescriptor> = ColumnFamily::all()
            .iter()
            .map(|cf| ColumnFamilyDescriptor::new(cf.name(), Options::default()))
            .collect();

        let inner = DB::open_cf_descriptors(&db_opts, &cfg.path, cf_descriptors)?;
        Ok(Self { inner })
    }

    /// Resolve a column-family handle. Returns `Error::UnknownColumn` if
    /// the open handle didn't include it.
    pub fn cf(&self, cf: ColumnFamily) -> Result<&rocksdb::ColumnFamily> {
        self.inner
            .cf_handle(cf.name())
            .ok_or(Error::UnknownColumn(cf.name()))
    }

    /// Raw put helper.
    pub fn put_raw(&self, cf: ColumnFamily, key: &[u8], value: &[u8]) -> Result<()> {
        let h = self.cf(cf)?;
        self.inner.put_cf(h, key, value)?;
        Ok(())
    }

    /// Raw get helper.
    pub fn get_raw(&self, cf: ColumnFamily, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let h = self.cf(cf)?;
        Ok(self.inner.get_cf(h, key)?)
    }

    /// Borrow the underlying DB. Stores use this for batch writes.
    pub(crate) fn inner(&self) -> &DB {
        &self.inner
    }

    /// Drop and remove the on-disk directory. **Tests only.**
    #[cfg(test)]
    pub fn destroy_for_tests(path: impl AsRef<std::path::Path>) {
        let _ = DB::destroy(&Options::default(), path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_creates_all_column_families() {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 64,
        };
        let db = Database::open(&cfg).unwrap();
        for cf in ColumnFamily::all() {
            db.cf(*cf).expect("every CF must be present");
        }
    }

    #[test]
    fn put_then_get_round_trip() {
        let dir = tempdir().unwrap();
        let cfg = StorageConfig {
            path: dir.path().to_path_buf(),
            create_if_missing: true,
            max_total_wal_size_mb: 64,
        };
        let db = Database::open(&cfg).unwrap();
        db.put_raw(ColumnFamily::MacroQc, b"k", b"v").unwrap();
        let got = db.get_raw(ColumnFamily::MacroQc, b"k").unwrap();
        assert_eq!(got.as_deref(), Some(b"v".as_slice()));
    }
}
