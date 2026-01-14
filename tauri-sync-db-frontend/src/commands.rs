//! Cloud Sync Command Wrappers
//!
//! Frontend bindings for cloud synchronization commands.

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use js_sys::Promise;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    fn invoke(cmd: &str, args: JsValue) -> Promise;
}

/// Sync configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    pub token: String,
}

/// Configure cloud synchronization
pub async fn configure_cloud_sync(url: String, token: String) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        url: String,
        token: String,
    }
    
    let args = serde_wasm_bindgen::to_value(&Args { url, token })
        .map_err(|e| format!("Serialization error: {}", e))?;
    
    let promise = invoke("configure_cloud_sync", args);
    let result = JsFuture::from(promise).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{:?}", e)))?;
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Response error: {}", e))
}

/// Get current sync configuration
pub async fn get_cloud_sync_config() -> Result<Option<SyncConfig>, String> {
    let promise = invoke("get_cloud_sync_config", JsValue::NULL);
    let result = JsFuture::from(promise).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{:?}", e)))?;
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Response error: {}", e))
}

/// Save sync configuration without triggering sync
pub async fn save_cloud_sync_config(url: String, token: String) -> Result<(), String> {
    #[derive(Serialize)]
    struct Args {
        url: String,
        token: String,
    }
    
    let args = serde_wasm_bindgen::to_value(&Args { url, token })
        .map_err(|e| format!("Serialization error: {}", e))?;
    
    let promise = invoke("save_cloud_sync_config", args);
    let result = JsFuture::from(promise).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{:?}", e)))?;
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Response error: {}", e))
}

/// Manually trigger database sync
pub async fn sync_cloud_db() -> Result<(), String> {
    let promise = invoke("sync_cloud_db", JsValue::NULL);
    let result = JsFuture::from(promise).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{:?}", e)))?;
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Response error: {}", e))
}

/// Check if cloud sync is enabled for current session
pub async fn is_cloud_sync_enabled() -> bool {
    let promise = invoke("is_cloud_sync_enabled", JsValue::NULL);
    match JsFuture::from(promise).await {
        Ok(result) => serde_wasm_bindgen::from_value(result).unwrap_or(false),
        Err(_) => false
    }
}
