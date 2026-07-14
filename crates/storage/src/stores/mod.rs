//! Type-specific stores. Each module exposes encode-and-put / decode-on-get
//! helpers built on top of [`crate::db::Database`].

pub mod blob_chunk_store;
pub mod blob_publish_store;
pub mod blob_status_store;
pub mod macro_store;
pub mod micro_store;
pub mod slash_store;
pub mod valset_store;
pub mod vertex_store;
pub mod vote_book_store;
