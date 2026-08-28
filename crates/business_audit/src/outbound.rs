//! Outbound persistence adapters.

mod pg_repo;
mod read_repo;

pub use pg_repo::insert_with_tx;
pub use read_repo::{detail, export_with_tx, list};
