//! Generic Mobile Bottom Navigation Components
//!
//! Reusable navigation components for mobile apps with sync functionality.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

/// Helper to invoke Tauri commands safely
async fn invoke_safe(cmd: &str, args: JsValue) -> Result<JsValue, String> {
    use wasm_bindgen::prelude::*;
    
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
        async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    }
    
    invoke(cmd, args).await.map_err(|e| {
        e.as_string().unwrap_or_else(|| format!("{:?}", e))
    })
}

/// Sync button state
#[derive(Clone, Copy, PartialEq)]
pub enum SyncState {
    Idle,      // ☁️
    Syncing,   // 🔄
    Success,   // ✅
    Error,     // ❌
}

/// Reusable sync button component
#[component]
pub fn SyncButton() -> impl IntoView {
    let sync_state = RwSignal::new(SyncState::Idle);
    
    let do_sync = move |_| {
        if sync_state.get() == SyncState::Syncing {
            return; // Prevent double-click
        }
        
        sync_state.set(SyncState::Syncing);
        
        spawn_local(async move {
            match invoke_safe("manual_sync", JsValue::NULL).await {
                Ok(_) => {
                    sync_state.set(SyncState::Success);
                    // Reset to idle after 2 seconds
                    set_timeout(move || sync_state.set(SyncState::Idle), std::time::Duration::from_secs(2));
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("同步失败: {}", e).into());
                    sync_state.set(SyncState::Error);
                    // Reset to idle after 2 seconds
                    set_timeout(move || sync_state.set(SyncState::Idle), std::time::Duration::from_secs(2));
                }
            }
        });
    };
    
    view! {
        <button
            class="mobile-nav-item"
            on:click=do_sync
            disabled=move || sync_state.get() == SyncState::Syncing
        >
            <div class=move || format!("mobile-nav-icon{}", if sync_state.get() == SyncState::Syncing { " spinning" } else { "" })>
                {move || match sync_state.get() {
                    SyncState::Idle => "☁️",
                    SyncState::Syncing => "🔄",
                    SyncState::Success => "✅",
                    SyncState::Error => "❌",
                }}
            </div>
            <div class="mobile-nav-label">"同步"</div>
        </button>
    }
}

/// Settings button component
#[component]
pub fn SettingsButton(
    #[prop(into)] is_active: Signal<bool>,
    on_click: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <button
            class=move || if is_active.get() { "mobile-nav-item active" } else { "mobile-nav-item" }
            on:click=move |_| on_click()
        >
            <div class="mobile-nav-icon">"⚙️"</div>
            <div class="mobile-nav-label">"设置"</div>
        </button>
    }
}

/// Generic bottom navigation bar
/// 
/// Accepts children elements to be rendered as custom navigation buttons
#[component]
pub fn GenericBottomNav(
    /// Optional callback when settings button is clicked
    #[prop(optional)]
    on_settings_click: Option<Box<dyn Fn()>>,
    /// Custom navigation buttons (left side)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let (show_settings, set_show_settings) = signal(false);
    
    let handle_settings_click = move |_| {
        set_show_settings.update(|v| *v = !*v);
        if let Some(ref callback) = on_settings_click {
            callback();
        }
    };
    
    view! {
        <div class="mobile-bottom-nav">
            // Custom buttons on the left
            {children.map(|c| c())}
            
            // Sync button in the middle
            <SyncButton />
            
            // Settings button on the right
            <button
                class=move || if show_settings.get() { "mobile-nav-item active" } else { "mobile-nav-item" }
                on:click=handle_settings_click
            >
                <div class="mobile-nav-icon">"⚙️"</div>
                <div class="mobile-nav-label">"设置"</div>
            </button>
        </div>
    }
}
