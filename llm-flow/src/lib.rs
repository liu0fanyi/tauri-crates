#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct RecommendItem {
    pub name: String,
    pub score: f32,
    pub source: String,
}

pub async fn generate_tags_llm(
    title: String,
    labels: Vec<String>,
    top_k: usize,
    threshold: f32,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<Vec<RecommendItem>, String> {
    use async_openai::config::OpenAIConfig;
    use async_openai::types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    };
    use async_openai::Client;

    let api_key = std::env::var("SILICONFLOW_API_KEY")
        .map_err(|_| "SILICONFLOW_API_KEY not set".to_string())?;
    let base = base_url.unwrap_or_else(|| {
        std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.siliconflow.cn/v1".to_string())
    });
    let model_name = model.unwrap_or_else(|| {
        // std::env::var("LLM_MODEL").unwrap_or_else(|_| "deepseek-ai/DeepSeek-V3.2-Exp".to_string())
        std::env::var("LLM_MODEL").unwrap_or_else(|_| "Qwen/Qwen3-VL-32B-Instruct".to_string())
    });

    let cfg = OpenAIConfig::new()
        .with_api_base(&base)
        .with_api_key(api_key);
    let client = Client::with_config(cfg);

    let lname = title.to_lowercase();
    let tokens: Vec<&str> = lname
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let mut scored: Vec<(String, i32)> = Vec::new();
    for l in &labels {
        let ln = l.to_lowercase();
        let mut s = 0;
        if !ln.is_empty() {
            if lname.contains(&ln) {
                s += 10;
            }
            if tokens.iter().any(|w| *w == ln) {
                s += 8;
            }
            if lname.starts_with(&ln) || lname.ends_with(&ln) {
                s += 4;
            }
        }
        if s > 0 {
            scored.push((l.clone(), s));
        } else {
            scored.push((l.clone(), 0));
        }
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    let max_send = core::cmp::min(scored.len(), 20);
    let preview = scored
        .iter()
        .take(max_send)
        .map(|(l, s)| format!("{}:{}", l, s))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("[LLM-FLOW] text prelabel weights [{}]", preview);
    let labels_to_send: Vec<String> = scored.into_iter().take(max_send).map(|(l, _)| l).collect();

    let sys = ChatCompletionRequestMessage::System(
        ChatCompletionRequestSystemMessageArgs::default()
            .content("你是一个文本标题标签推荐助手。输入是文件标题（纯文本），只从已存在的标签列表中挑选，尽可能返回多个（最多 top_k），并给出置信度。严格输出 JSON：{\"items\":[{\"name\":string,\"confidence\":number}]}. 不要创建新标签、不要包含除 JSON 外的任何文本。")
            .build()
            .map_err(|e| e.to_string())?,
    );
    let user_content = format!(
        "title: {}\nlabels: {}\n要求：只从 labels 中选择，最多 {} 个。",
        title,
        serde_json::to_string(&labels_to_send).unwrap_or_default(),
        top_k
    );
    let user = ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
            .content(user_content)
            .build()
            .map_err(|e| e.to_string())?,
    );
    let req = CreateChatCompletionRequestArgs::default()
        .model(model_name.clone())
        .temperature(0.0)
        .messages(vec![sys, user])
        .build()
        .map_err(|e| e.to_string())?;

    let timeout_secs: u64 = std::env::var("LLM_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(45);
    eprintln!(
        "[LLM-FLOW] text request model='{}' base='{}' labels_sent={} title_len={} timeout={}s",
        model_name,
        base,
        labels_to_send.len(),
        title.len(),
        timeout_secs,
    );
    let start = std::time::Instant::now();
    let resp = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        client.chat().create(req),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => {
            eprintln!("[LLM-FLOW] text timeout after {}s", timeout_secs);
            return Err("LLM request timeout".to_string());
        }
    };
    eprintln!(
        "[LLM-FLOW] text response in {}ms",
        start.elapsed().as_millis()
    );
    let mut out: Vec<RecommendItem> = Vec::new();
    if let Some(choice) = resp.choices.first() {
        if let Some(content) = &choice.message.content {
            let raw = content.clone();
            eprintln!("[LLM-FLOW] text raw content {} bytes", raw.len());
            let v = match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(val) => val,
                Err(_) => {
                    let mut s = raw.replace("```json", "").replace("```", "");
                    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
                        s = s[start..=end].to_string();
                    }
                    serde_json::from_str::<serde_json::Value>(&s)
                        .unwrap_or_else(|_| serde_json::json!({"items": []}))
                }
            };
            if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
                let mut raw_pairs: Vec<(String, f32)> = Vec::new();
                for it in items {
                    let name = it
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let confidence =
                        it.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    raw_pairs.push((name.clone(), confidence));
                    if !labels.iter().any(|l| l == &name) {
                        continue;
                    }
                    out.push(RecommendItem {
                        name,
                        score: confidence,
                        source: "llm".to_string(),
                    });
                }
                eprintln!(
                    "[LLM-FLOW] text raw items [{}]",
                    raw_pairs
                        .iter()
                        .map(|(n, c)| format!("{}:{:.3}", n, c))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }
    let before = out
        .iter()
        .map(|ri| format!("{}:{:.3}", ri.name, ri.score))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("[LLM-FLOW] text allowed items [{}]", before);
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    let final_out: Vec<RecommendItem> = out
        .into_iter()
        .filter(|x| x.score >= threshold)
        .take(top_k)
        .collect();
    let final_str = final_out
        .iter()
        .map(|ri| format!("{}:{:.3}", ri.name, ri.score))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "[LLM-FLOW] text selected items [{}] threshold={:.2} top_k={}",
        final_str, threshold, top_k
    );
    Ok(final_out)
}

pub async fn generate_image_tags_llm(
    image_path: String,
    labels: Vec<String>,
    top_k: usize,
    threshold: f32,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<Vec<RecommendItem>, String> {
    use async_openai::config::OpenAIConfig;
    use async_openai::types::{
        ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPart,
        ChatCompletionRequestMessageContentPartImageArgs,
        ChatCompletionRequestMessageContentPartTextArgs, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ImageUrlArgs,
    };
    use async_openai::Client;

    let api_key = std::env::var("SILICONFLOW_API_KEY")
        .map_err(|_| "SILICONFLOW_API_KEY not set".to_string())?;
    let base = base_url.unwrap_or_else(|| {
        std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.siliconflow.cn/v1".to_string())
    });
    let model_name = model.unwrap_or_else(|| {
        // std::env::var("LLM_MODEL").unwrap_or_else(|_| "deepseek-ai/deepseek-vl2".to_string())
        std::env::var("LLM_MODEL").unwrap_or_else(|_| "Qwen/Qwen3-VL-32B-Instruct".to_string())
    });

    let bytes = std::fs::read(&image_path).map_err(|e| e.to_string())?;
    let mime = {
        let p = std::path::Path::new(&image_path);
        match p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        }
    };
    let data_url = {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        format!("data:{};base64,{}", mime, STANDARD.encode(&bytes))
    };

    let cfg = OpenAIConfig::new()
        .with_api_base(&base)
        .with_api_key(api_key);
    let client = Client::with_config(cfg);

    let sys = ChatCompletionRequestMessage::System(
        ChatCompletionRequestSystemMessageArgs::default()
            .content("你是一个图片标签推荐助手。只从已存在的标签列表中挑选，尽可能返回多个（最多 top_k），并给出置信度。严格输出 JSON：{\"items\":[{\"name\":string,\"confidence\":number}]}. 不要创建新标签、不要包含除 JSON 外的任何文本。")
            .build()
            .map_err(|e| e.to_string())?,
    );
    let text_part = ChatCompletionRequestMessageContentPart::Text(
        ChatCompletionRequestMessageContentPartTextArgs::default()
            .text(format!(
                "labels: {}\n最多选择 {} 个，只从 labels 中选择。",
                serde_json::to_string(&labels).unwrap_or_default(),
                top_k
            ))
            .build()
            .unwrap(),
    );
    let image_part = ChatCompletionRequestMessageContentPart::Image(
        ChatCompletionRequestMessageContentPartImageArgs::default()
            .image_url(ImageUrlArgs::default().url(data_url).build().unwrap())
            .build()
            .unwrap(),
    );
    let user = ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
            .content(vec![text_part, image_part])
            .build()
            .map_err(|e| e.to_string())?,
    );
    let req = CreateChatCompletionRequestArgs::default()
        .model(model_name.clone())
        .temperature(0.0)
        .messages(vec![sys, user])
        .build()
        .map_err(|e| e.to_string())?;

    let v_timeout_secs: u64 = std::env::var("LLM_VISION_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    eprintln!(
        "[LLM-FLOW] vision request model='{}' base='{}' bytes={} labels_sent={} timeout={}s",
        model_name,
        base,
        bytes.len(),
        labels.len(),
        v_timeout_secs,
    );
    let v_start = std::time::Instant::now();
    let resp = match tokio::time::timeout(
        std::time::Duration::from_secs(v_timeout_secs),
        client.chat().create(req),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => {
            eprintln!("[LLM-FLOW] vision timeout after {}s", v_timeout_secs);
            return Err("LLM vision request timeout".to_string());
        }
    };
    eprintln!(
        "[LLM-FLOW] vision response in {}ms",
        v_start.elapsed().as_millis()
    );
    let mut out: Vec<RecommendItem> = Vec::new();
    if let Some(choice) = resp.choices.first() {
        if let Some(content) = &choice.message.content {
            let raw = content.clone();
            let v = match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(val) => val,
                Err(_) => {
                    let mut s = raw.replace("```json", "").replace("```", "");
                    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
                        s = s[start..=end].to_string();
                    }
                    serde_json::from_str::<serde_json::Value>(&s)
                        .unwrap_or_else(|_| serde_json::json!({"items": []}))
                }
            };
            if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
                let mut allowed = std::collections::HashSet::new();
                for l in &labels {
                    allowed.insert(l.to_lowercase());
                }
                for it in items {
                    let name = it
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !allowed.contains(&name.to_lowercase()) {
                        continue;
                    }
                    let confidence =
                        it.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    out.push(RecommendItem {
                        name,
                        score: confidence,
                        source: "llm-vision".to_string(),
                    });
                }
            }
        }
    }
    if out.is_empty() {
        let stem = std::path::Path::new(&image_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let tokens: Vec<&str> = stem
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        let mut scored: Vec<(String, i32)> = Vec::new();
        for l in &labels {
            let ln = l.to_lowercase();
            let mut s = 0;
            if !ln.is_empty() {
                if stem.contains(&ln) {
                    s += 10;
                }
                if tokens.iter().any(|w| *w == ln) {
                    s += 8;
                }
                if stem.starts_with(&ln) || stem.ends_with(&ln) {
                    s += 4;
                }
            }
            if s > 0 {
                scored.push((l.clone(), s));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        for (name, _) in scored.into_iter().take(top_k) {
            out.push(RecommendItem {
                name,
                score: 0.0,
                source: "rule".to_string(),
            });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(out
        .into_iter()
        .filter(|x| x.score >= threshold)
        .take(top_k)
        .collect())
}
