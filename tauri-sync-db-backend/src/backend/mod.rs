//! Backend synchronization logic for libsql/Turso
//!
//! Provides database initialization, cloud sync configuration, and connection management.

use std::sync::Arc;
use libsql::{Builder, Connection, Database};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::Mutex;

/// Sync configuration for Turso cloud database
#[derive(Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    pub token: String,
}

/// Database state wrapper
#[derive(Clone)]
pub struct DbState {
    db: Arc<Mutex<Option<Arc<Database>>>>,
    conn: Arc<Mutex<Option<Connection>>>,
    /// Whether cloud sync is enabled for this session
    is_sync_enabled: Arc<Mutex<bool>>,
    /// Current sync URL (for logging)
    sync_url: Arc<Mutex<String>>,
}

impl DbState {
    pub fn new() -> Self {
        Self {
            db: Arc::new(Mutex::new(None)),
            conn: Arc::new(Mutex::new(None)),
            is_sync_enabled: Arc::new(Mutex::new(false)),
            sync_url: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Check if cloud sync is enabled for this session
    pub async fn is_cloud_sync_enabled(&self) -> bool {
        *self.is_sync_enabled.lock().await
    }

    /// Set sync enabled status and URL
    pub async fn set_sync_config(&self, enabled: bool, url: String) {
        *self.is_sync_enabled.lock().await = enabled;
        *self.sync_url.lock().await = url;
    }

    /// Get current sync URL
    pub async fn get_sync_url(&self) -> String {
        self.sync_url.lock().await.clone()
    }

    /// Get a connection, initializing if necessary
    pub async fn get_connection(&self) -> Result<Connection, String> {
        let guard = self.conn.lock().await;
        if let Some(conn) = &*guard {
            return Ok(conn.clone());
        }
        Err("Database not initialized".to_string())
    }

    /// Manually trigger database sync (for cloud-synced databases)
    pub async fn sync(&self) -> Result<(), String> {
        let guard = self.db.lock().await;
        if let Some(db) = &*guard {
            db.sync().await.map_err(|e| {
                let err_str = format!("{}", e);
                if err_str.contains("File mode") || err_str.contains("not supported") {
                    "云同步未启用。请先配置云同步并重启应用。".to_string()
                } else {
                    format!("同步失败: {}", e)
                }
            })?;
            Ok(())
        } else {
            Err("数据库未初始化".to_string())
        }
    }

    /// Close all connections and drop database
    pub async fn close(&self) {
        let mut db_guard = self.db.lock().await;
        let mut conn_guard = self.conn.lock().await;
        *conn_guard = None;
        *db_guard = None;
    }

    /// Update this DbState's internals from another DbState (for async initialization)
    pub async fn update_from(&self, other: &DbState) {
        eprintln!("DbState::update_from: Starting state transfer");
        let other_db = other.db.lock().await;
        let other_conn = other.conn.lock().await;
        let other_sync_enabled = other.is_sync_enabled.lock().await;
        let other_sync_url = other.sync_url.lock().await;

        eprintln!("DbState::update_from: Acquired locks, other_db is_some={}, other_conn is_some={}", 
                 other_db.is_some(), other_conn.is_some());

        *self.db.lock().await = other_db.clone();
        *self.conn.lock().await = other_conn.clone();
        *self.is_sync_enabled.lock().await = *other_sync_enabled;
        *self.sync_url.lock().await = other_sync_url.clone();
        
        eprintln!("DbState::update_from: State transfer completed");
    }
}

/// Get sync configuration file path
fn get_config_path(db_path: &PathBuf) -> PathBuf {
    db_path.parent().unwrap().join("sync_config.json")
}

/// Load sync configuration from file
fn load_config(db_path: &PathBuf) -> Option<SyncConfig> {
    let path = get_config_path(db_path);
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            return serde_json::from_str(&content).ok();
        }
    }
    None
}

/// Validate cloud connection with Turso
pub async fn validate_cloud_connection(url: String, token: String) -> Result<(), String> {
    log::info!("Validating cloud connection: url={}", url);

    // Basic format check
    if !url.starts_with("libsql://") && !url.starts_with("https://") {
        log::error!("Invalid URL format: {}", url);
        return Err("URL must start with libsql:// or https://".to_string());
    }

    // Convert libsql:// to https:// for HTTP check
    let http_url = if url.starts_with("libsql://") {
        url.replace("libsql://", "https://")
    } else {
        url.clone()
    };

    log::info!("HTTP URL: {}", http_url);
    log::info!("Token length: {}", token.len());

    // Use tauri-plugin-http's reqwest to avoid rustls-platform-verifier issues on Android
    let client = tauri_plugin_http::reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            log::error!("Failed to build HTTP client: {}", e);
            format!("Client build failed: {}", e)
        })?;

    // Standard LibSQL/Turso HTTP API expects POST with JSON statements
    let query_body = serde_json::json!({
        "statements": ["SELECT 1"]
    });

    log::info!("Sending validation request to: {}", http_url);

    let body_str = serde_json::to_string(&query_body).map_err(|e| e.to_string())?;

    let res = client.post(&http_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(body_str)
        .send()
        .await;

    let res = match res {
        Ok(r) => {
            log::info!("Request sent successfully");
            r
        }
        Err(e) => {
            log::error!("Request failed: {}", e);
            return Err(format!("Connection failed: {}", e));
        }
    };

    let status = res.status();
    log::info!("Response status: {}", status);

    if status == tauri_plugin_http::reqwest::StatusCode::UNAUTHORIZED 
        || status == tauri_plugin_http::reqwest::StatusCode::FORBIDDEN {
        log::error!("Authentication failed");
        return Err("Authentication failed (Invalid Token)".to_string());
    }

    if !status.is_success() {
        log::error!("Server returned error status: {}", status);
        return Err(format!("Server returned error: {}", status));
    }

    log::info!("Cloud connection validated successfully");
    Ok(())
}

/// Type alias for migration function
pub type MigrationFn = fn(&Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Initialize local database connection
async fn init_local_db_connection(db_path_str: &str) -> Result<(Database, Connection, bool, String), String> {
    let db = Builder::new_local(db_path_str)
        .build()
        .await
        .map_err(|e| format!("Failed to build local db: {}", e))?;
    let conn = db.connect().map_err(|e| format!("Failed to connect: {}", e))?;
    Ok((db, conn, false, String::new()))
}

/// Initialize cloud database with auto-recovery on conflict
async fn init_cloud_db_connection(db_path: &PathBuf, conf: SyncConfig) -> Result<(Database, Connection, bool, String), String> {
    let db_path_str = db_path.to_str().ok_or("Invalid DB path")?;
    let sync_url = conf.url.clone();
    eprintln!("Initializing Synced DB: {}, token len: {}", conf.url, conf.token.len());
    
    // Validate connection first
    let validation_result = validate_cloud_connection(conf.url.clone(), conf.token.clone()).await;
    
    if let Err(e) = validation_result {
        eprintln!("Cloud connection validation failed: {}", e);
        eprintln!("Falling back to local mode due to invalid configuration.");
        return init_local_db_connection(db_path_str).await;
    }

    // Try to initialize cloud connection
    async fn try_build_connect(path: &str, url: String, token: String) -> Result<(Database, Connection), String> {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();

        let db = Builder::new_synced_database(path, url, token)
            .connector(https)
            .build()
            .await
            .map_err(|e| format!("Build failed: {}", e))?;
        let conn = db.connect().map_err(|e| format!("Connect failed: {}", e))?;

        // Force initial sync to detect conflicts immediately
        db.sync().await.map_err(|e| format!("Initial sync failed: {}", e))?;

        Ok((db, conn))
    }

    match try_build_connect(db_path_str, conf.url.clone(), conf.token.clone()).await {
        Ok((db, conn)) => Ok((db, conn, true, sync_url.clone())),
        Err(e) => {
            eprintln!("Synced DB init failed: {}", e);

            // Check for various sync conflict conditions
            let should_recover = e.contains("local state is incorrect")
                || e.contains("invalid local state")
                || e.contains("server returned a conflict")
                || e.contains("Generation ID mismatch")
                || e.contains("mismatch")
                || e.contains("metadata file does not");

            eprintln!("Should auto-recover: {}", should_recover);

            if should_recover {
                eprintln!("Detected conflicting local DB state. Auto-recovering by wiping local DB...");

                // Backup conflicting database
                let conflict_path = db_path.with_extension("db.legacy");
                if conflict_path.exists() {
                    eprintln!("Removing old legacy backup: {:?}", conflict_path);
                    let _ = std::fs::remove_file(&conflict_path);
                }
                if let Err(e) = std::fs::rename(&db_path, &conflict_path) {
                    eprintln!("Rename to legacy failed: {} - removing instead", e);
                    let _ = std::fs::remove_file(&db_path);
                } else {
                    eprintln!("Backed up old DB to: {:?}", conflict_path);
                }

                // Clean up sync metadata
                eprintln!("Cleaning up sync metadata...");
                let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
                let _ = std::fs::remove_file(db_path.with_extension("db-shm"));

                let sync_dir = db_path.parent().unwrap().join(format!("{}-sync", db_path.file_name().unwrap().to_str().unwrap()));
                if sync_dir.exists() {
                    eprintln!("Removing sync directory: {:?}", sync_dir);
                    if sync_dir.is_dir() {
                        let _ = std::fs::remove_dir_all(&sync_dir);
                    } else {
                        let _ = std::fs::remove_file(&sync_dir);
                    }
                }

                eprintln!("Retrying with clean state...");
                // Retry with clean state
                match try_build_connect(db_path_str, conf.url, conf.token).await {
                    Ok((db, conn)) => Ok((db, conn, true, sync_url.clone())),
                    Err(e) => {
                        eprintln!("Retry failed after recovery: {}", e);
                        eprintln!("Falling back to local mode...");
                        init_local_db_connection(db_path_str).await
                    }
                }
            } else {
                eprintln!("Cloud init failed (non-recoverable): {}", e);
                eprintln!("Falling back to local mode...");
                init_local_db_connection(db_path_str).await
            }
        }
    }
}

/// Initialize database with custom migrations
pub async fn init_db<F>(db_path: &PathBuf, migrations_fn: F) -> Result<DbState, String>
where
    F: for<'a> FnOnce(&'a Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>>,
{
    let db_path_str = db_path.to_str().ok_or("Invalid DB path")?;

    let config = load_config(db_path);

    let (db, conn, is_cloud_sync, sync_url) = if let Some(conf) = config {
        // Only use cloud sync if BOTH url and token are non-empty
        if conf.url.is_empty() || conf.token.is_empty() {
            eprintln!("Sync config has empty URL or token, falling back to local mode");
            init_local_db_connection(db_path_str).await?
        } else {
            // Cloud sync mode
            let msg = format!("Initializing Synced DB: {}, token len: {}", conf.url, conf.token.len());
            eprintln!("{}", msg);

            init_cloud_db_connection(db_path, conf).await?
        }
    } else {
        // Local only mode
        init_local_db_connection(db_path_str).await?
    };

    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", ())
        .await
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    // Run migrations
    migrations_fn(&conn).await?;

    let state = DbState::new();
    *state.db.lock().await = Some(Arc::new(db));
    *state.conn.lock().await = Some(conn);
    state.set_sync_config(is_cloud_sync, sync_url).await;

    Ok(state)
}

/// Configure cloud sync with Turso database
pub async fn configure_sync(db_path: &PathBuf, url: String, token: String) -> Result<(), String> {
    let config = SyncConfig { url, token };
    let config_path = get_config_path(db_path);
    std::fs::write(config_path, serde_json::to_string(&config).unwrap())
        .map_err(|e| e.to_string())?;

    eprintln!("Sync config saved");
    Ok(())
}

/// Get current sync configuration
pub fn get_sync_config(db_path: &PathBuf) -> Option<SyncConfig> {
    load_config(db_path)
}
