//! Generic Synchronization Logic
//!
//! This module provides a reusable implementation of the generic syncing protocol.
//! Applications must implement the `SyncSchema` trait to define their specific tables.

use crate::backend::{DbState, query_strings, execute_sql};
use tauri_plugin_http::reqwest;
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait to define the schema for synchronization.
pub trait SyncSchema {
    /// List of table names to sync, in order.
    fn tables(&self) -> Vec<&str>;
    
    /// Get columns for a specific table.
    /// Should return a list of column names.
    fn get_columns(&self, table: &str) -> Vec<&str>;
    
    /// Get the primary key column names for a table.
    /// Returns a list of columns that form the primary key.
    fn get_pks(&self, table: &str) -> Vec<&str>;

    /// Get the type of a specific column.
    /// Returns the type string (e.g., "INTEGER", "TEXT") if validation is needed.
    fn get_column_type(&self, table: &str, col: &str) -> Option<String>;
}

#[derive(Debug, Serialize, Deserialize)]
struct TursoError {
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TursoResultSet {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum TursoItemResponse {
    Success { results: TursoResultSet },
    Error { error: TursoError },
}

#[derive(Debug, Serialize, Deserialize)]
struct TursoResponse {
    results: Vec<TursoItemResponse>,
}

// Internal unified result for our logic
struct TursoResult {
    rows: Option<Vec<Vec<Value>>>,
    error: Option<String>,
}

struct TursoResponseUnified {
    results: Vec<TursoResult>,
}

/// Orchestrates the full sync process for all tables.
pub async fn sync_all<S: SyncSchema + Send + Sync>(
    client: &reqwest::Client,
    state: &DbState,
    schema: &S,
    url: &str,
    token: &str,
) -> Result<(), String> {
    eprintln!("Starting cloud sync...");
    
    // 1. Verify remote schema
    ensure_remote_schema(client, schema, url, token).await?;
    
    let tables = schema.tables();
    let mut tasks = Vec::new();

    // 2. Parallelize sync for each table
    // We need to resolve the type checking issue.
    // Better approach: Extract column types map for each table before spawning.
    
    // Let's rewrite the loop slightly
    for table_name in tables {
        let table = table_name.to_string();
        let client = client.clone();
        let state = state.clone();
        let url = url.to_string();
        let token = token.to_string();
        
        let columns: Vec<String> = schema.get_columns(&table).iter().map(|s| s.to_string()).collect();
        let pks: Vec<String> = schema.get_pks(&table).iter().map(|s| s.to_string()).collect();
        let updated_at_type = schema.get_column_type(&table, "updated_at").unwrap_or("TEXT".to_string());
        
        tasks.push(tokio::spawn(async move {
            sync_table(&client, &state, &url, &token, &table, &columns, &pks, &updated_at_type).await
        }));
    }

    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => {
                if let Err(e) = result {
                    eprintln!("Table sync failed: {}", e);
                    errors.push(e);
                }
            }
            Err(e) => {
                eprintln!("Task join failed: {}", e);
                errors.push(e.to_string());
            }
        }
    }

    if !errors.is_empty() {
        return Err(format!("Sync completed with {} errors: {:?}", errors.len(), errors));
    }
    
    eprintln!("Cloud sync completed successfully.");
    Ok(())
}

async fn ensure_remote_schema<S: SyncSchema>(
    client: &reqwest::Client, 
    schema: &S, 
    url: &str, 
    token: &str
) -> Result<(), String> {
    eprintln!("[{}] Verifying remote schema (fast mode)...", chrono::Local::now().format("%H:%M:%S%.3f"));
    
    let mut tasks = Vec::new();
    let tables = schema.tables();

    // Check standard columns for ALL tables in the schema
    for table_name in tables {
        let table = table_name.to_string();
        
        // Define columns to ensure existence of
        let cols_to_check = vec!["updated_at", "created_at", "deleted_at"];
        
        for col_name in cols_to_check {
            // Only check if local schema has this column
            if let Some(col_type) = schema.get_column_type(&table, col_name) {
                let url = url.to_string();
                let token = token.to_string();
                let table = table.clone();
                let col = col_name.to_string();
                let client = client.clone();
                
                tasks.push(tokio::spawn(async move {
                    let default_val = if col_type.to_uppercase().contains("INT") {
                        "0"
                    } else {
                        "'1970-01-01T00:00:00'"
                    };
                    
                    let sql = format!("ALTER TABLE {} ADD COLUMN {} {} DEFAULT {}", 
                        table, col, col_type, default_val);
                    
                    // Ignore error (will fail if column exists)
                    let _ = execute_remote_query(&client, &url, &token, &sql).await;
                }));
            }
        }
    }
    
    for task in tasks {
        let _ = task.await;
    }
    
    eprintln!("[{}] Remote schema verification finished.", chrono::Local::now().format("%H:%M:%S%.3f"));
    Ok(())
}

async fn sync_table(
    client: &reqwest::Client, 
    state: &DbState, 
    url: &str, 
    token: &str, 
    table: &str,
    columns: &[String],
    pks: &[String],
    updated_at_type: &str
) -> Result<(), String> {
    eprintln!("Syncing table: {}", table);

    // Capture time AT START of sync
    let now = if updated_at_type.to_uppercase().contains("INT") {
         chrono::Local::now().timestamp_millis().to_string()
    } else {
         chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    };
    
    let conn_guard = state.get_connection().await.map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("Database not initialized")?;
    
    // Get last sync time
    let mut last_sync_time = if updated_at_type.to_uppercase().contains("INT") {
        "0".to_string()
    } else {
        "1970-01-01 00:00:00".to_string()
    };
    {
        let query = format!("SELECT last_sync_time FROM sync_status WHERE table_name = '{}'", table);
        if let Ok(rows) = query_strings(conn, &query) {
            if let Some(row) = rows.first() {
                 if let Some(Some(val)) = row.get(0) {
                     last_sync_time = val.clone();
                 }
            }
        }
    }
    
    // Fix: If we expect INT (millis) but got a Date String (from previous syncs), convert it.
    if updated_at_type.to_uppercase().contains("INT") {
        if let Err(_) = last_sync_time.parse::<i64>() {
            // Not a number, try parsing as date
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&last_sync_time, "%Y-%m-%d %H:%M:%S") {
                last_sync_time = dt.and_utc().timestamp_millis().to_string();
                eprintln!("Converting legacy date string '{}' to millis '{}' for table {}", dt, last_sync_time, table);
            } else {
                // If it fails, maybe it's just garbage or empty? Default to 0 is safer than SQL error.
                 eprintln!("Warning: Could not parse last_sync_time '{}' as int or date for table {}. Defaulting to 0.", last_sync_time, table);
                 last_sync_time = "0".to_string();
            }
        }
    }
    
    eprintln!("Last sync time for {}: {}", table, last_sync_time);
    
    // 1. PUSH
    drop(conn_guard); 
    
    push_changes(client, state, url, token, table, columns, pks, &last_sync_time, updated_at_type).await?;
    
    // 2. PULL
    pull_changes(client, state, url, token, table, columns, pks, &last_sync_time, updated_at_type).await?;
    
    // 3. Update sync status
    let conn_guard = state.get_connection().await.map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("Database not initialized")?;
    
    let sql = format!(
        "INSERT OR REPLACE INTO sync_status (table_name, last_sync_time, last_sync_direction, sync_count) 
         VALUES ('{}', '{}', 'both', COALESCE((SELECT sync_count FROM sync_status WHERE table_name = '{}') + 1, 1))", 
        table, now, table
    );
    execute_sql(conn, &sql).map_err(|e| e.to_string())?;
    
    Ok(())
}

async fn push_changes(
    client: &reqwest::Client, 
    state: &DbState, 
    url: &str, 
    token: &str, 
    table: &str, 
    columns: &[String],
    pks: &[String], 
    last_sync_time: &str,
    updated_at_type: &str
) -> Result<(), String> {
    let conn_guard = state.get_connection().await.map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("Database not initialized")?;

    if columns.is_empty() {
        return Ok(());
    }
    
    let col_list = columns.join(", ");
    
    let query = if updated_at_type.to_uppercase().contains("INT") {
        format!("SELECT {} FROM {} WHERE updated_at > {}", col_list, table, last_sync_time)
    } else {
        format!("SELECT {} FROM {} WHERE updated_at > '{}'", col_list, table, last_sync_time)
    };
    
    let rows = query_strings(conn, &query).map_err(|e| e.to_string())?;

    drop(conn_guard);
    
    if rows.is_empty() {
        return Ok(());
    }

    eprintln!("Pushing {} records for table {}", rows.len(), table);

    let mut statements = Vec::new();
    
    let update_set = columns.iter()
        .map(|c| format!("{} = excluded.{}", c, c))
        .collect::<Vec<_>>()
        .join(", ");

    for row in rows {
        let mut values = Vec::new();
        for val_opt in row {
            match val_opt {
                Some(v) => values.push(format!("'{}'", v.replace("'", "''"))),
                None => values.push("NULL".to_string()),
            }
        }
        
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT({}) DO UPDATE SET {} WHERE excluded.updated_at > {}.updated_at",
            table,
            col_list,
            values.join(", "),
            pks.join(", "),
            update_set,
            table
        );
        statements.push(sql);
    }
    
    execute_remote_batch(client, url, token, statements).await?;
    
    Ok(())
}

async fn pull_changes(
    client: &reqwest::Client, 
    state: &DbState, 
    url: &str, 
    token: &str, 
    table: &str, 
    columns: &[String],
    pks: &[String],
    last_sync_time: &str,
    updated_at_type: &str
) -> Result<(), String> {
    let col_list = columns.join(", ");
    
    let sql = if updated_at_type.to_uppercase().contains("INT") {
         format!("SELECT {} FROM {} WHERE updated_at > {}", col_list, table, last_sync_time)
    } else {
         format!("SELECT {} FROM {} WHERE updated_at > '{}'", col_list, table, last_sync_time)
    };
    
    let rows = fetch_remote_rows(client, url, token, &sql).await?;
    
    if rows.is_empty() {
        return Ok(());
    }
    
    eprintln!("Pulling {} records for table {}", rows.len(), table);
    
    let conn_guard = state.get_connection().await.map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("Database not initialized")?;
    
    let mut collision_count = 0;
    
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    
    for row in rows {
        let mut row_map = HashMap::new();
        for (i, col) in columns.iter().enumerate() {
            if let Some(val) = row.get(i).and_then(|v| v.clone()) {
                 row_map.insert(col.to_string(), val);
            }
        }
        
        let mut pk_conditions = Vec::new();
        for pk in pks {
             let pk_val = row_map.get(pk).cloned().unwrap_or_default();
             if pk_val.is_empty() { continue; } // This check might need refinement for composite keys if one part is empty/null but legal? but usually PK shouldn't be empty string.
             pk_conditions.push(format!("{} = '{}'", pk, pk_val.replace("'", "''")));
        }
        
        if pk_conditions.len() != pks.len() {
             // Skip if we couldn't find all PK values
             continue; 
        }

        let remote_updated_at = row_map.get("updated_at").cloned().unwrap_or_default();
        
        let mut should_update = true;
        {
            let where_clause = pk_conditions.join(" AND ");
            let check_sql = format!("SELECT updated_at FROM {} WHERE {}", table, where_clause);
            let mut stmt = tx.prepare(&check_sql).map_err(|e| e.to_string())?;
            if let Ok(local_updated) = stmt.query_row([], |r| r.get::<_, String>(0)) {
                if local_updated > remote_updated_at {
                    should_update = false;
                    collision_count += 1;
                }
            }
        }
        
        if should_update {
             let mut values = Vec::new();
             for val_opt in row {
                 match val_opt {
                     Some(v) => values.push(format!("'{}'", v.replace("'", "''"))),
                     None => values.push("NULL".to_string()),
                 }
             }
             
             let upsert_sql = format!(
                "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                table,
                col_list,
                values.join(", ")
            );
            tx.execute(&upsert_sql, []).map_err(|e| e.to_string())?;
        }
    }
    
    tx.commit().map_err(|e| e.to_string())?;
    
    if collision_count > 0 {
        eprintln!("Ignored {} remote updates due to newer local versions", collision_count);
    }
    
    Ok(())
}

async fn fetch_remote_rows(client: &reqwest::Client, url: &str, token: &str, sql: &str) -> Result<Vec<Vec<Option<String>>>, String> {
    let http_url = url.replace("libsql://", "https://");
    
    let response = client
        .post(http_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&json!({
            "statements": [sql]
        })).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
        
    let text = response.text().await.map_err(|e| e.to_string())?;
    
    let results: Vec<TursoItemResponse> = serde_json::from_str(&text).map_err(|e| format!("Parse error: {} (Body: {})", e, text))?;
    
    if let Some(first) = results.first() {
        match first {
            TursoItemResponse::Error { error } => Err(error.message.clone()),
            TursoItemResponse::Success { results } => {
                let mut data = Vec::new();
                for r in &results.rows {
                   let mut row_vec = Vec::new();
                   for cell in r {
                       match cell {
                           Value::Null => row_vec.push(None),
                           Value::String(s) => row_vec.push(Some(s.clone())),
                           Value::Number(n) => row_vec.push(Some(n.to_string())),
                           Value::Bool(b) => row_vec.push(Some(b.to_string())),
                           _ => row_vec.push(Some(cell.to_string())),
                       }
                   }
                   data.push(row_vec);
                }
                Ok(data)
            }
        }
    } else {
        Ok(Vec::new())
    }
}

pub async fn execute_remote_batch(client: &reqwest::Client, url: &str, token: &str, statements: Vec<String>) -> Result<(), String> {
    let http_url = url.replace("libsql://", "https://");
    
    let response = client
        .post(http_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&json!({
            "statements": statements
        })).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
        
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Server error: {} - {}", status, body));
    }
    
    let text = response.text().await.map_err(|e| format!("Failed to read response: {}", e))?;
        
    match serde_json::from_str::<Vec<TursoItemResponse>>(&text) {
        Ok(results) => {
            for (i, result) in results.iter().enumerate() {
                if let TursoItemResponse::Error { error } = result {
                    eprintln!("[{}] Error in batch statement {}: {}", chrono::Local::now().format("%H:%M:%S%.3f"), i, error.message);
                    return Err(format!("Batch statement {} failed: {}", i, error.message));
                }
            }
        },
        Err(e) => {
            eprintln!("[{}] Warning: Failed to parse batch response: {} (Body: {})", chrono::Local::now().format("%H:%M:%S%.3f"), e, text);
        }
    }
    
    Ok(())
}


async fn execute_remote_query(client: &reqwest::Client, url: &str, token: &str, stmt: &str) -> Result<TursoResponseUnified, String> {
    let http_url = url.replace("libsql://", "https://");
    
    let response = client
        .post(http_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&json!({
            "statements": [stmt]
        })).map_err(|e| e.to_string())?)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;
        
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Server error: {} - {}", status, body));
    }
    
    let text = response.text().await.map_err(|e| format!("Failed to read response: {}", e))?;
    
    let raw_results: Vec<TursoItemResponse> = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse response: {} (Body: {})", e, text))?;
        
    let mut results = Vec::new();
    for item in raw_results {
        match item {
            TursoItemResponse::Success { results: res } => {
                results.push(TursoResult {
                    rows: Some(res.rows),
                    error: None,
                });
            },
            TursoItemResponse::Error { error } => {
                results.push(TursoResult {
                    rows: None,
                    error: Some(error.message),
                });
            }
        }
    }
        
    Ok(TursoResponseUnified { results })
}

/// Struct to hold dynamically loaded schema information
pub struct DynamicSchema {
    tables: Vec<String>,
    table_info: HashMap<String, TableInfo>,
}

struct TableInfo {
    columns: Vec<String>,
    pks: Vec<String>,
    column_types: HashMap<String, String>,
}

impl DynamicSchema {
    /// Load schema from the database for the given list of tables.
    /// If tables list is empty, it could potentially discover all tables (optional future feature),
    /// but for now we expect a list of tables to include.
    pub async fn load(state: &DbState, target_tables: Vec<&str>) -> Result<Self, String> {
        let conn_guard = state.get_connection().await.map_err(|e| e.to_string())?;
        let conn = conn_guard.as_ref().ok_or("Database not initialized")?;
        
        let mut schema = DynamicSchema {
            tables: target_tables.iter().map(|s| s.to_string()).collect(),
            table_info: HashMap::new(),
        };


        
        // Refactored implementation below replacing the loop
        for table in target_tables {
            let mut columns = Vec::new();
            let mut pks = Vec::new();
            let mut column_types = HashMap::new();
            
            // pragma table_info returns: cid, name, type, notnull, dflt_value, pk
            let query = format!("PRAGMA table_info({})", table);
            let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
            
            struct ColumnMeta {
                name: String,
                col_type: String,
                pk_idx: i32,
            }
            
            let column_iter = stmt.query_map([], |row| {
                Ok(ColumnMeta {
                    name: row.get(1)?,
                    col_type: row.get(2)?,
                    pk_idx: row.get(5)?,
                })
            }).map_err(|e| e.to_string())?;

            let mut pk_cols = Vec::new();

            for col in column_iter {
                let col = col.map_err(|e| e.to_string())?;
                columns.push(col.name.clone());
                column_types.insert(col.name.clone(), col.col_type.clone());
                
                if col.pk_idx > 0 {
                    pk_cols.push(col);
                }
            }
            
            // Sort PKs by index (composite keys order matters)
            pk_cols.sort_by_key(|c| c.pk_idx);
            pks = pk_cols.into_iter().map(|c| c.name).collect();

            if pks.is_empty() {
                if columns.contains(&"id".to_string()) {
                    pks.push("id".to_string());
                }
            }

            schema.table_info.insert(table.to_string(), TableInfo { columns, pks, column_types });
        }
        
        Ok(schema)
    }
}

impl SyncSchema for DynamicSchema {
    fn tables(&self) -> Vec<&str> {
        self.tables.iter().map(|s| s.as_str()).collect()
    }
    
    fn get_columns(&self, table: &str) -> Vec<&str> {
        self.table_info.get(table)
            .map(|info| info.columns.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
    
    fn get_pks(&self, table: &str) -> Vec<&str> {
         self.table_info.get(table)
            .map(|info| info.pks.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    fn get_column_type(&self, table: &str, col: &str) -> Option<String> {
        self.table_info.get(table)
            .and_then(|info| info.column_types.get(col).cloned())
    }
}
