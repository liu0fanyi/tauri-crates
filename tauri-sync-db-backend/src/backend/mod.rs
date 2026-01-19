//! Backend synchronization logic (Rusqlite version)

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex; 
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs;
use tauri_plugin_http::reqwest;
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    pub token: String,
}

#[derive(Clone)]
pub struct DbState {
    // Use tokio Mutex for async compatibility
    pub conn: Arc<Mutex<Option<Connection>>>,
    pub db_path: PathBuf,
}

impl DbState {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
            db_path,
        }
    }

    pub async fn get_connection(&self) -> Result<tokio::sync::MutexGuard<'_, Option<Connection>>, String> {
        let guard = self.conn.lock().await;
        if guard.is_none() {
            return Err("Database not initialized".to_string());
        }
        Ok(guard)
    }
    
    /// Check if cloud sync is enabled (checked by presence of config)
    pub fn is_cloud_sync_enabled(&self) -> bool {
        get_sync_config(&self.db_path).is_some()
    }
}

/// Initialize database connection
pub async fn init_db(db_path: &PathBuf) -> Result<DbState, String> {
    eprintln!("Initializing DB at: {:?}", db_path);

    // Create directory if not exists
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    
    // Set some PRAGMAs for better performance/safety
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;"
    ).map_err(|e| e.to_string())?;

    let state = DbState {
        conn: Arc::new(Mutex::new(Some(conn))),
        db_path: db_path.clone(),
    };
    
    Ok(state)
}

/// Initialize local-only database (same as init_db for Rusqlite)
pub async fn init_local_only(db_path: &PathBuf) -> Result<DbState, String> {
    init_db(db_path).await
}

pub fn load_config(db_path: &Path) -> Option<SyncConfig> {
    let config_path = db_path.parent()?.join("sync_config.json");
    if config_path.exists() {
        let content = fs::read_to_string(config_path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

pub async fn configure_sync(db_path: &Path, url: String, token: String) -> Result<(), String> {
    let config = SyncConfig { url: url.clone(), token: token.clone() };
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    
    if let Some(parent) = db_path.parent() {
        let config_path = parent.join("sync_config.json");
        fs::write(config_path, json).map_err(|e| e.to_string())?;
        
        // Also validate connection if possible
        let _ = validate_cloud_connection(url, token).await; 
        
        Ok(())
    } else {
        Err("Invalid database path".to_string())
    }
}

pub fn get_sync_config(db_path: &PathBuf) -> Option<SyncConfig> {
    load_config(db_path)
}

pub fn execute_sql(conn: &Connection, sql: &str) -> Result<(), String> {
    conn.execute(sql, ()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Query and return rows as vector of optional strings
pub fn query_strings(conn: &Connection, sql: &str) -> Result<Vec<Vec<Option<String>>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let column_count = stmt.column_count();
    
    // Map each row to a Vec<Option<String>>
    let rows = stmt.query_map([], |row| {
        let mut row_vec = Vec::new();
        for i in 0..column_count {
            // Use rusqlite's dynamic value extraction
            let val = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => None,
                rusqlite::types::ValueRef::Integer(i) => Some(i.to_string()),
                rusqlite::types::ValueRef::Real(f) => Some(f.to_string()),
                rusqlite::types::ValueRef::Text(t) => Some(String::from_utf8_lossy(t).to_string()),
                rusqlite::types::ValueRef::Blob(_) => Some("<blob>".to_string()),
            };
            row_vec.push(val);
        }
        Ok(row_vec)
    }).map_err(|e| e.to_string())?;
    
    let mut results = Vec::new();
    for r in rows {
        results.push(r.map_err(|e| e.to_string())?);
    }
    
    Ok(results)
}

/// Validate connection to Turso (Cloud)
pub async fn validate_cloud_connection(url: String, token: String) -> Result<(), String> {
    eprintln!("Validating cloud connection to {}", url);
    let http_url = url.replace("libsql://", "https://");
    
    let client = reqwest::Client::new();
    
    // Simple query to check connection
    let body = json!({
        "statements": ["SELECT 1"]
    });
    
    let res = client.post(&http_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).map_err(|e| format!("Serialization failed: {}", e))?)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;
        
    if !res.status().is_success() {
        return Err(format!("Auth failed: {}", res.status()));
    }
    
    Ok(())
}
