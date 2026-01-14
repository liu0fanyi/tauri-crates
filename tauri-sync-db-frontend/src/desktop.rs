//! Cloud Sync Configuration Modal for Desktop/PC
//!
//! Compact dropdown modal dialog for configuring Turso cloud database synchronization.

use leptos::prelude::*;

/// Cloud sync configuration modal
#[component]
pub fn SyncModal(
    /// Whether to show the modal
    show: Signal<bool>,
    /// Set show state
    set_show: WriteSignal<bool>,
    /// Sync URL
    sync_url: Signal<String>,
    /// Set sync URL
    set_sync_url: WriteSignal<String>,
    /// Sync Token
    sync_token: Signal<String>,
    /// Set sync Token
    set_sync_token: WriteSignal<String>,
    /// Sync status
    sync_status: Signal<String>,
    /// Sync message
    sync_msg: Signal<String>,
    /// Save config callback (only saves, no sync)
    on_save_config: Callback<()>,
    /// Manual sync callback
    on_manual_sync: Callback<()>,
) -> impl IntoView {
    view! {
        {move || if show.get() {
            view! {
                // Backdrop to detect clicks outside
                <div 
                    style="position: fixed; top: 0; left: 0; width: 100%; height: 100%; z-index: 49; background: transparent;"
                    on:click=move |_| set_show.set(false)
                ></div>

                // Compact dropdown near sync button
                <div 
                    style="position: fixed; top: 40px; right: 120px; z-index: 50; width: 320px; background: white; border-radius: 6px; box-shadow: 0 4px 12px rgba(0,0,0,0.15); border: 1px solid #ddd;"
                >
                    // Header
                    <div style="padding: 8px 12px; border-bottom: 1px solid #e5e5e5; display: flex; justify-content: space-between; align-items: center; background: #f9f9f9;">
                        <span style="font-size: 14px; font-weight: 600;">"☁️ 云同步"</span>
                        <button
                            on:click=move |_| set_show.set(false)
                            style="background: none; border: none; color: #999; font-size: 20px; cursor: pointer; padding: 0; line-height: 1;"
                        >
                            "×"
                        </button>
                    </div>
                    
                    // Body
                    <div style="padding: 12px;">
                        <div style="margin-bottom: 12px;">
                            <label style="display: block; font-size: 12px; color: #666; margin-bottom: 4px;">"URL"</label>
                            <input
                                type="text"
                                style="width: 100%; padding: 6px 8px; border: 1px solid #ddd; border-radius: 4px; font-size: 13px; box-sizing: border-box;"
                                placeholder="libsql://..."
                                on:input=move |ev| set_sync_url.set(event_target_value(&ev))
                                prop:value=sync_url
                            />
                        </div>
                        
                        <div style="margin-bottom: 12px;">
                            <label style="display: block; font-size: 12px; color: #666; margin-bottom: 4px;">"Token"</label>
                            <input
                                type="password"
                                style="width: 100%; padding: 6px 8px; border: 1px solid #ddd; border-radius: 4px; font-size: 13px; box-sizing: border-box;"
                                placeholder="eyJ..."
                                on:input=move |ev| set_sync_token.set(event_target_value(&ev))
                                prop:value=sync_token
                            />
                        </div>
                        
                        // Status
                        {move || if !sync_msg.get().is_empty() {
                            let status = sync_status.get();
                            let (bg_color, text_color) = match status.as_str() {
                                "error" => ("#fee", "#c00"),
                                "success" => ("#efe", "#0a0"),
                                "testing" | "syncing" | "saving" => ("#eef", "#06c"),
                                _ => ("#f5f5f5", "#666")
                            };
                            view! {
                                <div style=format!("font-size: 12px; padding: 8px; border-radius: 4px; background: {}; color: {};", bg_color, text_color)>
                                    {sync_msg.get()}
                                </div>
                            }.into_any()
                        } else {
                            view! { <div></div> }.into_any()
                        }}
                    </div>
                    
                    // Footer with separate Save and Sync buttons
                    <div style="padding: 8px 12px; border-top: 1px solid #e5e5e5; background: #f9f9f9; display: flex; gap: 8px; justify-content: flex-end;">
                        <button
                            on:click=move |_| on_save_config.run(())
                            style="padding: 6px 12px; font-size: 13px; background: #6b7280; color: white; border: none; border-radius: 4px; cursor: pointer;"
                        >
                            "💾 保存"
                        </button>
                        <button
                            on:click=move |_| on_manual_sync.run(())
                            style="padding: 6px 12px; font-size: 13px; background: #2563eb; color: white; border: none; border-radius: 4px; cursor: pointer;"
                        >
                            "🔄 同步"
                        </button>
                    </div>
                </div>
            }.into_any()
        } else {
            view! { <div></div> }.into_any()
        }}
    }
}
