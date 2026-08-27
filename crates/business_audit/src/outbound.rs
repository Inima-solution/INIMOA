//! Outbound persistence adapters.

mod pg_repo;

pub use pg_repo::insert_with_tx;
