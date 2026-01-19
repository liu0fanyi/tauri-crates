//! Sync Settings for Mobile/Android
//!
//! Full-screen mobile sync configuration form with vertical layout.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

/// Helper to invoke Tauri commands safely
async fn invoke_safe(cmd: &str, args: JsValue) -> Result<JsValue, String> {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
        async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    }
    
    invoke(cmd, args).await.map_err(|e| {
        e.as_string().unwrap_or_else(|| format!("{:?}", e))
    })
}

/// Sync configuration data
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SyncConfig {
    pub url: String,
    pub token: String,
}

/// Mobile sync settings form component
#[component]
pub fn SyncSettingsForm(
    on_back: impl Fn() + 'static + Copy,
) -> impl IntoView {
    let url = RwSignal::new(String::new());
    let token = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let is_error = RwSignal::new(false);
    let is_configured = RwSignal::new(false);
    let is_syncing = RwSignal::new(false);
    let has_legacy = RwSignal::new(false);
    let is_migrating = RwSignal::new(false);

    // Load existing config on mount
    create_effect(move |_| {
        spawn_local(async move {
            // Check sync config
            match invoke_safe("get_sync_config", JsValue::NULL).await {
                Ok(result) => {
                    if let Ok(Some(c)) = serde_wasm_bindgen::from_value::<Option<SyncConfig>>(result) {
                        url.set(c.url);
                        token.set(c.token);
                        is_configured.set(true);
                    }
                }
                Err(_) => {}
            }
            // Check legacy database
            if let Ok(result) = invoke_safe("has_legacy_db", JsValue::NULL).await {
                if let Ok(has) = serde_wasm_bindgen::from_value::<bool>(result) {
                    has_legacy.set(has);
                }
            }
        });
    });

    // Save configuration
    let save_config = move |_| {
        message.set(String::new());
        
        let url_val = url.get();
        let token_val = token.get();
        
        if url_val.is_empty() || token_val.is_empty() {
            message.set("请填写 URL 和 Token".to_string());
            is_error.set(true);
            return;
        }
        
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "url": url_val,
                "token": token_val,
            })).unwrap();
            
            match invoke_safe("configure_sync", args).await {
                Ok(_) => {
                    message.set("配置已保存！请重启应用以使用云同步。".to_string());
                    is_error.set(false);
                    is_configured.set(true);
                }
                Err(e) => {
                    message.set(format!("保存失败: {}", e));
                    is_error.set(true);
                }
            }
        });
    };

    // Trigger sync
    let do_sync = move |_| {
        message.set(String::new());
        is_syncing.set(true);
        
        spawn_local(async move {
            match invoke_safe("manual_sync", JsValue::NULL).await {
                Ok(_) => {
                    message.set("同步成功！".to_string());
                    is_error.set(false);
                }
                Err(e) => {
                    message.set(e);
                    is_error.set(true);
                }
            }
            is_syncing.set(false);
        });
    };

    // Migrate from legacy
    let do_migrate = move |_| {
        message.set(String::new());
        is_migrating.set(true);
        
        spawn_local(async move {
            match invoke_safe("migrate_from_legacy", JsValue::NULL).await {
                Ok(result) => {
                    if let Ok(msg) = serde_wasm_bindgen::from_value::<String>(result) {
                        message.set(msg);
                    } else {
                        message.set("迁移完成！".to_string());
                    }
                    is_error.set(false);
                    has_legacy.set(false);
                }
                Err(e) => {
                    message.set(e);
                    is_error.set(true);
                }
            }
            is_migrating.set(false);
        });
    };

    view! {
        <div class="mobile-form-view">
            // Header
            <h2 style="margin: 0; font-size: 18px; padding: 12px 16px; background: white; border-bottom: 1px solid #e0e0e0; text-align: center;">"云同步设置"</h2>
            
            // Message display
            {move || {
                let msg = message.get();
                if msg.is_empty() {
                    None
                } else {
                    let style = if is_error.get() {
                        "padding: 12px; margin: 8px 16px; background: #fee; color: #c00; border-radius: 8px;"
                    } else {
                        "padding: 12px; margin: 8px 16px; background: #d4edda; color: #155724; border-radius: 8px;"
                    };
                    Some(view! {
                        <div style=style>{msg}</div>
                    })
                }
            }}
            
            // Form content
            <div style="padding: 16px; flex: 1; overflow-y: auto;">
                // Status indicator
                <div style="margin-bottom: 16px; padding: 12px; background: #f8f9fa; border-radius: 8px;">
                    <div style="font-size: 14px; color: #666;">
                        "同步状态: "
                        {move || if is_configured.get() { 
                            view! { <span style="color: #28a745;">"已配置"</span> }.into_any()
                        } else { 
                            view! { <span style="color: #dc3545;">"未配置"</span> }.into_any()
                        }}
                    </div>
                </div>
                
                // Turso URL
                <div style="margin-bottom: 16px;">
                    <label style="display: block; margin-bottom: 8px; font-weight: 500;">"Turso URL"</label>
                    <input
                        type="text"
                        placeholder="libsql://your-db.turso.io"
                        value=url
                        on:input=move |ev| url.set(event_target_value(&ev))
                        style="width: 100%; padding: 12px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; box-sizing: border-box;"
                    />
                </div>
                
                // Token
                <div style="margin-bottom: 16px;">
                    <label style="display: block; margin-bottom: 8px; font-weight: 500;">"Auth Token"</label>
                    <input
                        type="password"
                        placeholder="eyJhbGciOiJFZ..."
                        value=token
                        on:input=move |ev| token.set(event_target_value(&ev))
                        style="width: 100%; padding: 12px; border: 1px solid #ddd; border-radius: 8px; font-size: 14px; box-sizing: border-box;"
                    />
                </div>
                
                // Help text
                <div style="margin-bottom: 16px; padding: 12px; background: #e7f3ff; border-radius: 8px; font-size: 13px; color: #0066cc;">
                    <p style="margin: 0 0 8px 0;">"💡 提示"</p>
                    <p style="margin: 0;">"在 turso.tech 创建数据库后，可获取 URL 和 Token。保存配置后需要重启应用才能生效。"</p>
                </div>
            </div>
            
            // Bottom buttons
            <div style="padding: 16px; background: white; border-top: 1px solid #e0e0e0;">
                <button 
                    on:click=save_config
                    style="width: 100%; padding: 14px; background: #3b82f6; color: white; border: none; border-radius: 8px; font-size: 16px; font-weight: bold; margin-bottom: 8px;"
                >
                    "保存配置"
                </button>
                
                <button 
                    on:click=do_sync
                    disabled=move || is_syncing.get() || !is_configured.get()
                    style=move || format!(
                        "width: 100%; padding: 14px; border: 2px solid #3b82f6; border-radius: 8px; font-size: 16px; font-weight: bold; margin-bottom: 8px; {}",
                        if is_syncing.get() || !is_configured.get() {
                            "background: #f0f0f0; color: #999; border-color: #ddd;"
                        } else {
                            "background: white; color: #3b82f6;"
                        }
                    )
                >
                    {move || if is_syncing.get() { "同步中..." } else { "立即同步" }}
                </button>
                
                // Migrate from legacy button
                {move || if has_legacy.get() {
                    Some(view! {
                        <button 
                            on:click=do_migrate
                            disabled=move || is_migrating.get()
                            style=move || format!(
                                "width: 100%; padding: 14px; border: 2px solid #f59e0b; border-radius: 8px; font-size: 16px; font-weight: bold; {}",
                                if is_migrating.get() {
                                    "background: #f0f0f0; color: #999; border-color: #ddd;"
                                } else {
                                    "background: #fffbeb; color: #b45309;"
                                }
                            )
                        >
                            {move || if is_migrating.get() { "迁移中..." } else { "📥 从旧数据库迁移" }}
                        </button>
                    })
                } else {
                    None
                }}
            </div>
        </div>
    }
}
