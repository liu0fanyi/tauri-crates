use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

pub struct UpdateState {
    pub pending: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub has_update: bool,
    pub download_url: Option<String>,
}

pub async fn check(app_handle: AppHandle) -> Result<UpdateInfo, String> {
    let current = app_handle.package_info().version.to_string();
    let updater = app_handle.updater().map_err(|e| e.to_string())?;
    
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            // Try to get download URL if available (best effort)
            // Note: Currently tauri_plugin_updater::Update doesn't publically expose raw body or url easily 
            // without using format specific extractors, but we can try to assume standard usage or leave it empty for now if not accessible.
            // Wait, we can't get it easily. Let's return None for now or try to fetch it if possible.
            // Actually, we can just return empty string if we can't find it.
            Ok(UpdateInfo { 
                current, 
                latest: Some(update.version.clone()), 
                has_update: true,
                download_url: None // Placeholder, or we can try to parse it
            }) 
        },
        None => Ok(UpdateInfo { 
            current, 
            latest: None, 
            has_update: false, 
            download_url: None 
        }),
    }
}

pub async fn download_update(app_handle: AppHandle) -> Result<(), String> {
    rolling_logger::info("[updater] download_update() called");
    let updater = app_handle.updater().map_err(|e| e.to_string())?;
    
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        rolling_logger::info(&format!("[updater] Update found: {}, starting download...", update.version));
        let app = app_handle.clone();
        
        // Notify start
        let _ = app.emit("tauri-update-download-start", ());

        let bytes = update
            .download(
                |received: usize, total: Option<u64>| {
                    let _ = app.emit("tauri-update-progress", serde_json::json!({"received": received, "total": total}));
                },
                || {},
            )
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                rolling_logger::error(&format!("[updater] Download failed: {}", error_msg));
                let _ = app.emit("tauri-update-error", serde_json::json!({"error": error_msg.clone()}));
                error_msg
            })?;
        
        rolling_logger::info(&format!("[updater] Download complete, {} bytes.", bytes.len()));
        
        // Store in state
        let state = app_handle.state::<UpdateState>();
        let mut pending = state.pending.lock().unwrap();
        *pending = Some((update, bytes));
        
        // Notify complete
        let _ = app_handle.emit("tauri-update-complete", ());
        
        Ok(())
    } else {
        Err("No update available to download".to_string())
    }
}

pub async fn install_pending_update(app_handle: AppHandle) -> Result<(), String> {
    rolling_logger::info("[updater] install_pending_update() called");
    
    let state = app_handle.state::<UpdateState>();
    // Take the update from state
    let option = {
        let mut pending = state.pending.lock().unwrap();
        pending.take()
    };

    if let Some((update, bytes)) = option {
        rolling_logger::info(&format!("[updater] Installing pending update: {}", update.version));
        
        update.install(bytes).map_err(|e| {
            rolling_logger::error(&format!("[updater] Install failed: {}", e));
            e.to_string()
        })?;
        
        rolling_logger::info("[updater] Install initiated, restarting app...");
        app_handle.restart();
        Ok(())
    } else {
        Err("No pending update found in memory".to_string())
    }
}

pub async fn install(app_handle: AppHandle) -> Result<(), String> {
    // Legacy function support (or direct install)
    download_update(app_handle.clone()).await?;
    install_pending_update(app_handle).await
}

