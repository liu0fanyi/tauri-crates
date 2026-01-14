//! Tauri Sync DB Backend - Shared synchronization library
//!
//! Provides shared synchronization infrastructure for Tauri applications
//! using libsql with Turso cloud database support.
//!
//! Native-only crate (not compiled for WASM).

pub mod backend;

// Re-export commonly used types
pub use backend::{DbState, SyncConfig, init_db, configure_sync, get_sync_config, validate_cloud_connection};
