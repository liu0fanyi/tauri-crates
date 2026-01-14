# Tauri Apps - Shared Crates

This repository contains shared Rust crates used across multiple Tauri applications.

## Crates

### tauri-sync-db-backend
Backend components for cloud database synchronization with Turso (libsql).

**Features:**
- Database initialization and management
- Cloud sync configuration and validation
- Cross-platform support (Desktop + Android)
- Uses `tauri-plugin-http` for Android compatibility

**Usage:**
```rust
use tauri_sync_db_backend::{init_db, DbState};

let db_state = init_db(&db_path).await?;
let connection = db_state.get_connection().await?;
```

---

### tauri-sync-db-frontend
Frontend UI components for sync configuration (WASM-only).

**Features:**
- Mobile sync settings form with legacy migration support
- Desktop sync modal (compact dropdown)
- Generic bottom navigation with sync button
- Configurable settings button

**Usage:**
```rust
use tauri_sync_db_frontend::{SyncSettingsForm, GenericBottomNav, SyncModal};

// Mobile
view! { <SyncSettingsForm on_back=move || {} /> }

// Desktop
view! { <SyncModal show=show set_show=set_show ... /> }
```

---

### rolling-logger
Rolling file logger with circular buffer for Tauri applications.

**Features:**
- 10MB circular buffer (no unbounded growth)
- Android support with configurable log tags
- Tracing integration
- Desktop file logging with stderr output

**Usage:**
```rust
use rolling_logger::init_logger;

init_logger(log_dir, "MyApp")?;
tracing::info!("Hello, world!");
```

---

### leptos-dragdrop
Drag-and-drop utilities for Leptos 0.8 applications.

**Features:**
- Mouse-based drag-and-drop
- WASM-compatible
- Leptos reactive integration

## Development

Each crate is independently versioned and can be imported via path dependency:

```toml
[dependencies]
tauri-sync-db-backend = { path = "../crates/tauri-sync-db-backend" }
tauri-sync-db-frontend = { path = "../crates/tauri-sync-db-frontend" }
rolling-logger = { path = "../crates/rolling-logger" }
leptos-dragdrop = { path = "../crates/leptos-dragdrop" }
```

## License

MIT
