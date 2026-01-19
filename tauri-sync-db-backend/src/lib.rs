//! Tauri Sync DB Backend - Shared synchronization library
//!
//! Provides shared synchronization infrastructure for Tauri applications
//! using libsql with Turso cloud database support.
//!
//! Native-only crate (not compiled for WASM).

pub mod backend;
pub mod sync;

// Re-export commonly used types
pub use backend::{DbState, SyncConfig, init_db, init_local_only, configure_sync, get_sync_config, validate_cloud_connection, load_config, execute_sql, query_strings};
pub use sync::{SyncSchema, sync_all};

