//! Tauri Sync DB Frontend Components
//!
//! Shared UI components for sync configuration and management

pub mod commands;
pub mod desktop;
pub mod mobile;
pub mod mobile_nav;

// Re-export commonly used components
pub use mobile::SyncSettingsForm;
pub use mobile_nav::{GenericBottomNav, SyncButton, SettingsButton, SyncState};
