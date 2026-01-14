use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::console;
use js_sys::{Function, Promise, Reflect};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct RecommendItem { pub name: String, pub score: f32, pub source: String }

pub async fn generate_for_file(
    file_path: String,
    labels: Vec<String>,
    top_k: usize,
    threshold: f32,
    base_url: Option<String>,
    model: Option<String>,
) -> Vec<RecommendItem> {
    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    console::log_1(&format!("[RECO] start file='{}' ext='{}' labels={}, top_k={} threshold={}", file_path, ext, labels.len(), top_k, threshold).into());
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
        let win = web_sys::window().unwrap();
        let tauri = Reflect::get(&win, &JsValue::from_str("__TAURI__")).unwrap();
        let core = Reflect::get(&tauri, &JsValue::from_str("core")).unwrap();
        let invoke_fn = Reflect::get(&core, &JsValue::from_str("invoke")).unwrap().dyn_into::<Function>().unwrap();
        let promise_val = invoke_fn.call2(&core, &JsValue::from_str(cmd), &args).unwrap();
        let promise = promise_val.dyn_into::<Promise>().unwrap();
        wasm_bindgen_futures::JsFuture::from(promise).await
    }
    if ["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct VisionArgs { image_path: String, labels: Vec<String>, top_k: usize, threshold: f32, base_url: Option<String>, model: Option<String> }
        let args = VisionArgs { image_path: file_path.clone(), labels, top_k, threshold, base_url, model };
        let val = match tauri_invoke("generate_image_tags_llm", serde_wasm_bindgen::to_value(&args).unwrap()).await {
            Ok(v) => v,
            Err(e) => { console::error_1(&format!("[RECO] vision invoke error: {:?}", e).into()); return vec![] }
        };
        match serde_wasm_bindgen::from_value::<Vec<RecommendItem>>(val) {
            Ok(list) => { console::log_1(&format!("[RECO] vision items=[{}]", list.iter().map(|ri| format!("{}:{:.3}:{}", ri.name, ri.score, ri.source)).collect::<Vec<_>>().join(", ")).into()); list }
            Err(e) => { console::error_1(&format!("[RECO] vision parse error: {}", e).into()); vec![] }
        }
    } else {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct LlmArgs { title: String, labels: Vec<String>, top_k: usize, threshold: f32, base_url: Option<String>, model: Option<String> }
        let title = std::path::Path::new(&file_path).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if title.is_empty() { return vec![]; }
        let args = LlmArgs { title, labels, top_k, threshold, base_url, model };
        let val = match tauri_invoke("generate_tags_llm", serde_wasm_bindgen::to_value(&args).unwrap()).await {
            Ok(v) => v,
            Err(e) => { console::error_1(&format!("[RECO] llm invoke error: {:?}", e).into()); return vec![] }
        };
        match serde_wasm_bindgen::from_value::<Vec<RecommendItem>>(val) {
            Ok(list) => { console::log_1(&format!("[RECO] llm items=[{}]", list.iter().map(|ri| format!("{}:{:.3}:{}", ri.name, ri.score, ri.source)).collect::<Vec<_>>().join(", ")).into()); list }
            Err(e) => { console::error_1(&format!("[RECO] llm parse error: {}", e).into()); vec![] }
        }
    }
}
