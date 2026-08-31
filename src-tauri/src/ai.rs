use base64::Engine;
use futures_util::StreamExt;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, RgbaImage};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

use super::{config_snapshot, AppState, CharacterCard};

const SERVICE_NAME: &str = "com.ifan.sakipet";
const AI_DIRECTORY: &str = "ai";
const MAX_MESSAGE_CHARS: usize = 4_000;
const MAX_HISTORY_MESSAGES: usize = 200;
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static LAST_HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_VISION_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderKind {
    OpenaiResponses,
    AnthropicMessages,
    OpenaiCompatible,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ModelEndpointConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub credential_ref: Option<String>,
    pub max_output_tokens: u32,
}

impl Default for ModelEndpointConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::OpenaiResponses,
            base_url: "https://api.openai.com/v1".to_string(),
            model: String::new(),
            credential_ref: None,
            max_output_tokens: 300,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct AiSettings {
    pub enabled: bool,
    pub chat_model: Option<ModelEndpointConfig>,
    pub vision_model: Option<ModelEndpointConfig>,
    pub memory_enabled: bool,
    pub max_recent_messages: usize,
    pub heartbeat_enabled: bool,
    pub heartbeat_min_minutes: u32,
    pub heartbeat_max_minutes: u32,
    pub heartbeat_vision_chance: f64,
    pub desktop_vision_enabled: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            chat_model: None,
            vision_model: None,
            memory_enabled: true,
            max_recent_messages: 12,
            heartbeat_enabled: true,
            heartbeat_min_minutes: 20,
            heartbeat_max_minutes: 60,
            heartbeat_vision_chance: 0.3,
            desktop_vision_enabled: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
    pub source: String,
    pub vision_summary: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct MemoryFact {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub scope: String,
    pub importance: f64,
    pub confidence: f64,
    pub created_at: u64,
    pub updated_at: u64,
    pub status: String,
}

impl Default for MemoryFact {
    fn default() -> Self {
        Self {
            id: String::new(),
            content: String::new(),
            kind: "fact".to_string(),
            scope: "pet".to_string(),
            importance: 0.5,
            confidence: 0.5,
            created_at: now_ms(),
            updated_at: now_ms(),
            status: "active".to_string(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ChatHistoryResponse {
    pub pet_id: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatStarted {
    pub request_id: String,
}

#[derive(Default)]
pub(crate) struct AiRuntime {
    pub tasks: Mutex<HashMap<String, (String, tauri::async_runtime::JoinHandle<()>)>>,
    pub active_pets: Mutex<HashMap<String, String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatDeltaEvent {
    request_id: String,
    pet_id: String,
    delta: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatCompleteEvent {
    request_id: String,
    pet_id: String,
    message: ChatMessage,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatErrorEvent {
    request_id: String,
    pet_id: String,
    message: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn request_id() -> String {
    format!(
        "req-{}-{}",
        now_ms(),
        REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Capture only the display containing the cursor. The screenshot is encoded
/// directly into memory and is never written to a temporary file.
pub(crate) async fn capture_desktop_data_url(app: &tauri::AppHandle) -> Result<String, String> {
    let cursor = app
        .cursor_position()
        .map_err(|error| format!("无法读取鼠标所在显示器: {error}"))?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || capture_desktop_sync(&app, cursor.x, cursor.y))
        .await
        .map_err(|error| format!("桌面截图任务失败: {error}"))?
}

#[allow(unused_mut, unused_variables)]
fn encode_screenshot(
    mut image: RgbaImage,
    monitor_origin: (i32, i32),
    pixel_scale: f64,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    mask_sakipet_macos(&mut image, monitor_origin, pixel_scale, app);

    #[cfg(target_os = "windows")]
    mask_sakipet_windows(&mut image, monitor_origin, pixel_scale, app);

    let dynamic = DynamicImage::ImageRgba8(image);
    let longest = dynamic.width().max(dynamic.height());
    let resized = if longest > 1280 {
        dynamic.thumbnail(1280, 1280)
    } else {
        dynamic
    };
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 70)
        .encode_image(&resized)
        .map_err(|error| format!("压缩桌面截图失败: {error}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(target_os = "macos")]
fn capture_desktop_sync(
    app: &tauri::AppHandle,
    cursor_x: f64,
    cursor_y: f64,
) -> Result<String, String> {
    let monitor = xcap::Monitor::from_point(cursor_x.round() as i32, cursor_y.round() as i32)
        .map_err(|error| format!("无法找到鼠标所在显示器: {error}"))?;
    let origin = (
        monitor.x().map_err(|error| error.to_string())?,
        monitor.y().map_err(|error| error.to_string())?,
    );
    let pixel_scale = monitor.scale_factor().unwrap_or(1.0).max(1.0) as f64;
    let image = monitor
        .capture_image()
        .map_err(|error| format!("捕获桌面失败，请检查屏幕录制权限: {error}"))?;
    encode_screenshot(image, origin, pixel_scale, app)
}

#[cfg(target_os = "windows")]
fn capture_desktop_sync(
    app: &tauri::AppHandle,
    cursor_x: f64,
    cursor_y: f64,
) -> Result<String, String> {
    let monitor = xcap::Monitor::from_point(cursor_x.round() as i32, cursor_y.round() as i32)
        .map_err(|error| format!("无法找到鼠标所在显示器: {error}"))?;
    let origin = (
        monitor.x().map_err(|error| error.to_string())?,
        monitor.y().map_err(|error| error.to_string())?,
    );
    let image = monitor
        .capture_image()
        .map_err(|error| format!("捕获桌面失败，请检查屏幕捕获权限: {error}"))?;
    encode_screenshot(image, origin, 1.0, app)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_desktop_sync(
    app: &tauri::AppHandle,
    _cursor_x: f64,
    _cursor_y: f64,
) -> Result<String, String> {
    let _ = app;
    Err("当前平台暂不支持桌面视觉".to_string())
}

#[cfg(target_os = "windows")]
fn mask_sakipet_windows(
    image: &mut RgbaImage,
    monitor_origin: (i32, i32),
    pixel_scale: f64,
    app: &tauri::AppHandle,
) {
    for (label, window) in app.webview_windows() {
        if label != "main" && !label.starts_with("pet-") && label != "pet-manager" {
            continue;
        }
        if !window.is_visible().unwrap_or(false) {
            continue;
        }
        let Ok(position) = window.outer_position() else {
            continue;
        };
        let Ok(size) = window.outer_size() else {
            continue;
        };
        let left = ((position.x - monitor_origin.0) as f64 * pixel_scale).floor() as i32;
        let top = ((position.y - monitor_origin.1) as f64 * pixel_scale).floor() as i32;
        let right = ((position.x - monitor_origin.0 + size.width as i32) as f64 * pixel_scale)
            .ceil() as i32;
        let bottom = ((position.y - monitor_origin.1 + size.height as i32) as f64 * pixel_scale)
            .ceil() as i32;
        let x_start = left.max(0) as u32;
        let y_start = top.max(0) as u32;
        let x_end = right.min(image.width() as i32).max(0) as u32;
        let y_end = bottom.min(image.height() as i32).max(0) as u32;
        for y in y_start..y_end {
            for x in x_start..x_end {
                *image.get_pixel_mut(x, y) = image::Rgba([0, 0, 0, 255]);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn mask_sakipet_macos(
    image: &mut RgbaImage,
    monitor_origin: (i32, i32),
    pixel_scale: f64,
    _app: &tauri::AppHandle,
) {
    let process_id = std::process::id();
    let windows = xcap::Window::all().unwrap_or_default();
    for window in windows {
        if window.pid().ok() != Some(process_id) || window.is_minimized().unwrap_or(true) {
            continue;
        }
        let Ok(x) = window.x() else { continue };
        let Ok(y) = window.y() else { continue };
        let Ok(width) = window.width() else { continue };
        let Ok(height) = window.height() else {
            continue;
        };
        let left = ((x - monitor_origin.0) as f64 * pixel_scale).floor() as i32;
        let top = ((y - monitor_origin.1) as f64 * pixel_scale).floor() as i32;
        let right = ((x - monitor_origin.0 + width as i32) as f64 * pixel_scale).ceil() as i32;
        let bottom = ((y - monitor_origin.1 + height as i32) as f64 * pixel_scale).ceil() as i32;
        let x_start = left.max(0) as u32;
        let y_start = top.max(0) as u32;
        let x_end = right.min(image.width() as i32).max(0) as u32;
        let y_end = bottom.min(image.height() as i32).max(0) as u32;
        for y in y_start..y_end {
            for x in x_start..x_end {
                *image.get_pixel_mut(x, y) = image::Rgba([0, 0, 0, 255]);
            }
        }
    }
}

fn app_ai_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(AI_DIRECTORY))
        .map_err(|error| format!("无法定位 AI 数据目录: {error}"))
}

fn pet_ai_path(app: &tauri::AppHandle, pet_id: &str) -> Result<PathBuf, String> {
    if !super::is_safe_id(pet_id) {
        return Err("宠物 id 无效".to_string());
    }
    Ok(app_ai_path(app)?.join("pets").join(pet_id))
}

fn append_jsonl<T: Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建 AI 数据目录: {error}"))?;
    }
    let line = serde_json::to_string(value).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("无法写入 AI 数据: {error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("无法写入 AI 数据: {error}"))
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Vec<T> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn messages_path(app: &tauri::AppHandle, pet_id: &str) -> Result<PathBuf, String> {
    Ok(pet_ai_path(app, pet_id)?.join("messages.jsonl"))
}

fn memories_path(app: &tauri::AppHandle, pet_id: &str) -> Result<PathBuf, String> {
    Ok(pet_ai_path(app, pet_id)?.join("memories.jsonl"))
}

fn load_messages(app: &tauri::AppHandle, pet_id: &str) -> Result<Vec<ChatMessage>, String> {
    Ok(read_jsonl(messages_path(app, pet_id)?))
}

fn append_message(
    app: &tauri::AppHandle,
    pet_id: &str,
    message: &ChatMessage,
) -> Result<(), String> {
    append_jsonl(messages_path(app, pet_id)?, message)
}

fn load_memories(app: &tauri::AppHandle, pet_id: &str) -> Result<Vec<MemoryFact>, String> {
    let mut values: HashMap<String, MemoryFact> = HashMap::new();
    for fact in read_jsonl::<MemoryFact>(memories_path(app, pet_id)?) {
        if !fact.id.is_empty() {
            values.insert(fact.id.clone(), fact);
        }
    }
    let mut facts = values
        .into_values()
        .filter(|fact| fact.status == "active")
        .collect::<Vec<_>>();
    facts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(facts)
}

fn get_secret(reference: &Option<String>) -> Result<Option<String>, String> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    let entry = keyring::Entry::new(SERVICE_NAME, reference)
        .map_err(|error| format!("无法访问系统密钥环: {error}"))?;
    match entry.get_password() {
        Ok(secret) if !secret.is_empty() => Ok(Some(secret)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取 API Key: {error}")),
    }
}

fn normalized_endpoint(config: &ModelEndpointConfig) -> Result<(String, Option<String>), String> {
    let base = config.base_url.trim().trim_end_matches('/');
    if base.is_empty() || config.model.trim().is_empty() {
        return Err("请先填写模型地址和模型名称".to_string());
    }
    let endpoint = match config.provider {
        ProviderKind::OpenaiResponses => format!("{base}/responses"),
        ProviderKind::AnthropicMessages => format!("{base}/messages"),
        ProviderKind::OpenaiCompatible => format!("{base}/chat/completions"),
    };
    Ok((endpoint, get_secret(&config.credential_ref)?))
}

fn normalize_endpoint_config(mut config: ModelEndpointConfig) -> ModelEndpointConfig {
    config.base_url = config.base_url.trim().trim_end_matches('/').to_string();
    config.model = config.model.trim().to_string();
    config.max_output_tokens = config.max_output_tokens.clamp(1, 8_192);
    config
}

fn auth_headers(config: &ModelEndpointConfig, secret: Option<&str>) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    match config.provider {
        ProviderKind::AnthropicMessages => {
            if let Some(secret) = secret {
                headers.insert(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_str(secret).map_err(|error| error.to_string())?,
                );
            }
            headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static("2023-06-01"),
            );
        }
        ProviderKind::OpenaiResponses | ProviderKind::OpenaiCompatible => {
            if let Some(secret) = secret {
                let value = format!("Bearer {secret}");
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&value).map_err(|error| error.to_string())?,
                );
            }
        }
    }
    Ok(headers)
}

fn message_value(message: &ChatMessage) -> Value {
    json!({"role": message.role, "content": message.content})
}

fn prompt_for(
    card: &CharacterCard,
    pet_id: &str,
    profile: &str,
    memories: &[MemoryFact],
    summary: &str,
    query: &str,
) -> String {
    let mut prompt = String::from(
        "你是 SakiPet 桌面宠物。你只能进行聊天和陪伴，不执行文件、Shell、系统控制或网络工具。\n\n",
    );
    if !card.system_prompt.is_empty() {
        prompt.push_str(&card.system_prompt);
        prompt.push('\n');
    }
    for (label, value) in [
        ("角色描述", &card.description),
        ("性格", &card.personality),
        ("场景", &card.scenario),
        ("对话示例", &card.mes_example),
        ("初始问候", &card.first_mes),
    ] {
        if !value.is_empty() {
            prompt.push_str(&format!("\n{label}：\n{value}\n"));
        }
    }
    let lore = card.relevant_lorebook(&format!("{query}\n{summary}"));
    if !lore.is_empty() {
        prompt.push_str("\n相关世界书：\n");
        prompt.push_str(&lore);
    }
    if !profile.is_empty() {
        prompt.push_str(&format!("\n共享用户资料：\n{profile}\n"));
    }
    if !memories.is_empty() {
        prompt.push_str("\n关于你们共同经历的记忆：\n");
        for memory in memories.iter().take(8) {
            prompt.push_str(&format!("- {}\n", memory.content));
        }
    }
    if !summary.is_empty() {
        prompt.push_str(&format!("\n较早对话摘要：\n{summary}\n"));
    }
    prompt.push_str(&format!("\n当前宠物 ID：{pet_id}\n回复要求：使用自然简短的中文，保持角色语气，不要随机描述表情或动作。"));
    if !card.post_history_instructions.is_empty() {
        prompt.push_str(&format!(
            "\n\n历史后指令：\n{}",
            card.post_history_instructions
        ));
    }
    prompt
}

fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut result = Vec::new();
    for size in [1usize, 2, 3] {
        result.extend(chars.windows(size).map(|window| window.iter().collect()));
    }
    result.extend(text.split_whitespace().map(str::to_lowercase));
    result
}

fn relevant_memories(memories: &[MemoryFact], query: &str) -> Vec<MemoryFact> {
    let query_tokens = tokens(query);
    let now = now_ms();
    let mut scored: Vec<(f64, MemoryFact)> = memories
        .iter()
        .cloned()
        .map(|memory| {
            let memory_tokens = tokens(&memory.content);
            let overlap = query_tokens
                .iter()
                .filter(|token| memory_tokens.contains(token))
                .count() as f64;
            let age_days = now.saturating_sub(memory.updated_at) as f64 / 86_400_000.0;
            let recency = 1.0 / (1.0 + age_days / 30.0);
            let score = overlap * 2.0 + memory.importance * 1.5 + memory.confidence + recency;
            (score, memory)
        })
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    scored
        .into_iter()
        .take(8)
        .map(|(_, memory)| memory)
        .collect()
}

fn history_for_prompt(messages: &[ChatMessage], max_recent: usize) -> Vec<ChatMessage> {
    let start = messages.len().saturating_sub(max_recent.max(2));
    messages[start..].to_vec()
}

fn build_payload(
    config: &ModelEndpointConfig,
    prompt: &str,
    messages: &[ChatMessage],
    image_data_url: Option<&str>,
    stream: bool,
) -> Value {
    match config.provider {
        ProviderKind::OpenaiResponses => {
            let mut input: Vec<Value> = messages.iter().map(message_value).collect();
            if let Some(image) = image_data_url {
                input.push(json!({
                    "role":"user",
                    "content":[
                        {"type":"input_text","text":"请只描述你观察到的桌面内容，不要复述密码、令牌或联系方式。"},
                        {"type":"input_image","image_url":image,"detail":"low"}
                    ]
                }));
            }
            json!({
                "model": config.model,
                "instructions": prompt,
                "input": input,
                "max_output_tokens": config.max_output_tokens,
                "stream": stream,
                "store": false
            })
        }
        ProviderKind::AnthropicMessages => {
            let mut history: Vec<Value> = messages.iter().map(message_value).collect();
            if let Some(image) = image_data_url {
                let encoded = image
                    .split_once(',')
                    .map(|(_, value)| value)
                    .unwrap_or(image);
                history.push(json!({"role":"user","content":[
                    {"type":"text","text":"请只描述你观察到的桌面内容，不要复述密码、令牌或联系方式。"},
                    {"type":"image","source":{"type":"base64","media_type":"image/jpeg","data":encoded}}
                ]}));
            }
            json!({
                "model": config.model,
                "system": prompt,
                "messages": history,
                "max_tokens": config.max_output_tokens,
                "stream": stream
            })
        }
        ProviderKind::OpenaiCompatible => {
            let mut history = vec![json!({"role":"system","content":prompt})];
            history.extend(messages.iter().map(|message| {
                if image_data_url.is_some() && message.id == "__vision__" {
                    json!({"role":"user","content":[
                        {"type":"text","text":"请只描述你观察到的桌面内容，不要复述密码、令牌或联系方式。"},
                        {"type":"image_url","image_url":{"url":image_data_url.unwrap()}}
                    ]})
                } else { message_value(message) }
            }));
            json!({
                "model": config.model,
                "messages": history,
                "max_tokens": config.max_output_tokens,
                "stream": stream
            })
        }
    }
}

fn extract_text(value: &Value, provider: &ProviderKind) -> String {
    match provider {
        ProviderKind::OpenaiResponses => value
            .get("output_text")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .pointer("/output/0/content/0/text")
                    .and_then(Value::as_str)
            })
            .unwrap_or_default()
            .to_string(),
        ProviderKind::AnthropicMessages => value
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ProviderKind::OpenaiCompatible => value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn stream_delta(value: &Value, provider: &ProviderKind) -> Option<String> {
    match provider {
        ProviderKind::OpenaiResponses
            if value.get("type").and_then(Value::as_str) == Some("response.output_text.delta") =>
        {
            value
                .get("delta")
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        ProviderKind::AnthropicMessages
            if value.get("type").and_then(Value::as_str) == Some("content_block_delta") =>
        {
            value
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        ProviderKind::OpenaiCompatible => value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

async fn response_text(
    response: reqwest::Response,
    provider: &ProviderKind,
    mut on_delta: impl FnMut(String),
) -> Result<String, String> {
    let mut bytes = response.bytes_stream();
    let mut buffer = String::new();
    let mut text = String::new();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|error| format!("读取模型响应失败: {error}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((index, separator_len)) = sse_boundary(&buffer) {
            let event = buffer[..index].to_string();
            buffer.drain(..index + separator_len);
            for line in event.lines().filter_map(|line| line.strip_prefix("data:")) {
                let data = line.trim();
                if data == "[DONE]" {
                    continue;
                }
                let value: Value = match serde_json::from_str(data) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                if let Some(delta) = stream_delta(&value, provider) {
                    text.push_str(&delta);
                    on_delta(delta);
                }
            }
        }
    }
    for line in buffer.lines().filter_map(|line| line.strip_prefix("data:")) {
        let data = line.trim();
        if data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(delta) = stream_delta(&value, provider) {
            text.push_str(&delta);
            on_delta(delta);
        } else if text.is_empty() {
            let fallback = extract_text(&value, provider);
            if !fallback.is_empty() {
                text = fallback;
            }
        }
    }
    if text.is_empty() && !buffer.trim().is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(buffer.trim()) {
            text = extract_text(&value, provider);
        }
    }
    Ok(text)
}

fn sse_boundary(buffer: &str) -> Option<(usize, usize)> {
    match (buffer.find("\r\n\r\n"), buffer.find("\n\n")) {
        (Some(crlf), Some(lf)) if crlf < lf => Some((crlf, 4)),
        (Some(crlf), _) => Some((crlf, 4)),
        (_, Some(lf)) => Some((lf, 2)),
        _ => None,
    }
}

async fn call_stream(
    client: &reqwest::Client,
    config: &ModelEndpointConfig,
    prompt: &str,
    messages: &[ChatMessage],
    image_data_url: Option<&str>,
    stream: bool,
    on_delta: impl FnMut(String),
) -> Result<String, String> {
    let (endpoint, secret) = normalized_endpoint(config)?;
    if matches!(
        config.provider,
        ProviderKind::OpenaiResponses | ProviderKind::OpenaiCompatible
    ) && config.base_url.starts_with("https://api.openai.com")
        && secret.is_none()
    {
        return Err("未配置 OpenAI API Key".to_string());
    }
    if matches!(config.provider, ProviderKind::AnthropicMessages) && secret.is_none() {
        return Err("未配置 Anthropic API Key".to_string());
    }
    let mut request = client
        .post(endpoint)
        .headers(auth_headers(config, secret.as_deref())?)
        .json(&build_payload(
            config,
            prompt,
            messages,
            image_data_url,
            stream,
        ));
    if !stream {
        request = request.header(ACCEPT, "application/json");
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("连接模型失败: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "模型返回 HTTP {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        ));
    }
    if stream {
        response_text(response, &config.provider, on_delta).await
    } else {
        let value = response
            .json::<Value>()
            .await
            .map_err(|error| format!("解析模型响应失败: {error}"))?;
        Ok(extract_text(&value, &config.provider))
    }
}

fn card_for_pet(app: &tauri::AppHandle, pet_id: &str) -> Result<CharacterCard, String> {
    super::load_pet_character(app, pet_id)
}

fn profile_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_ai_path(app)?.join("profile.json"))
}

fn load_profile(app: &tauri::AppHandle) -> String {
    load_shared_memories(app)
        .into_iter()
        .filter(|fact| fact.status == "active")
        .map(|fact| format!("- {}", fact.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn load_shared_memories(app: &tauri::AppHandle) -> Vec<MemoryFact> {
    let path = profile_path(app).unwrap_or_default();
    fs::read_to_string(path)
        .ok()
        .and_then(|value| serde_json::from_str::<Vec<MemoryFact>>(&value).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|fact| !fact.id.is_empty())
        .collect::<Vec<_>>()
}

fn write_shared_memories(app: &tauri::AppHandle, facts: &[MemoryFact]) -> Result<(), String> {
    let path = profile_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(facts).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn append_shared_memory(app: &tauri::AppHandle, fact: &MemoryFact) -> Result<(), String> {
    let mut facts = load_shared_memories(app);
    if let Some(existing) = facts.iter_mut().find(|existing| existing.id == fact.id) {
        *existing = fact.clone();
        return write_shared_memories(app, &facts);
    }
    if facts
        .iter()
        .any(|existing| existing.status == "active" && existing.content == fact.content)
    {
        return Ok(());
    }
    facts.push(fact.clone());
    write_shared_memories(app, &facts)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SummaryFile {
    summary: String,
    updated_at: u64,
}

fn load_summary(app: &tauri::AppHandle, pet_id: &str) -> String {
    let path = pet_ai_path(app, pet_id)
        .unwrap_or_default()
        .join("summary.json");
    let Ok(value) = fs::read_to_string(path) else {
        return String::new();
    };
    serde_json::from_str::<SummaryFile>(&value)
        .map(|file| file.summary)
        .unwrap_or(value)
}

fn save_summary(app: &tauri::AppHandle, pet_id: &str, summary: &str) -> Result<(), String> {
    let path = pet_ai_path(app, pet_id)?.join("summary.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    let value = SummaryFile {
        summary: summary.to_string(),
        updated_at: now_ms(),
    };
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

fn choose_memories(app: &tauri::AppHandle, pet_id: &str, query: &str) -> Vec<MemoryFact> {
    relevant_memories(&load_memories(app, pet_id).unwrap_or_default(), query)
}

fn clean_reply(text: String, max_chars: usize) -> String {
    let text = text.trim().trim_matches('`').trim().to_string();
    if text.len() > max_chars {
        text.chars().take(max_chars).collect()
    } else {
        text
    }
}

fn extract_json(text: &str) -> Option<Value> {
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(clean).ok()
}

#[derive(Clone, Debug)]
struct MemoryOperation {
    action: String,
    target: String,
    content: String,
    kind: String,
    scope: String,
    importance: f64,
    confidence: f64,
}

async fn extract_memories(
    client: &reqwest::Client,
    config: &ModelEndpointConfig,
    messages: &[ChatMessage],
) -> Result<Vec<MemoryOperation>, String> {
    let prompt = "你是记忆提取器。只保留稳定、未来仍有用的用户资料或共同经历，不记录一次性闲聊。只返回 JSON：{\"facts\":[{\"action\":\"add|update|forget\",\"target\":\"要修改或遗忘的原记忆内容，可为空\",\"content\":\"新的记忆内容\",\"kind\":\"preference|profile|event\",\"scope\":\"shared|pet\",\"importance\":0.0,\"confidence\":0.0}]}。没有记忆就返回空数组。不要把普通寒暄写入记忆。";
    let value = call_stream(client, config, prompt, messages, None, false, |_| {}).await?;
    let Some(value) = extract_json(&value) else {
        return Ok(Vec::new());
    };
    Ok(value
        .get("facts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|fact| {
            let action = fact
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("add")
                .to_string();
            if !matches!(action.as_str(), "add" | "update" | "forget") {
                return None;
            }
            let content = fact
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let target = fact
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let target = if target.is_empty() && action == "forget" {
                content.clone()
            } else {
                target
            };
            if (action != "forget" && content.is_empty())
                || (action == "forget" && target.is_empty())
            {
                return None;
            }
            Some(MemoryOperation {
                action,
                target: target.chars().take(300).collect(),
                content: content.chars().take(300).collect(),
                kind: fact
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("fact")
                    .to_string(),
                scope: fact
                    .get("scope")
                    .and_then(Value::as_str)
                    .unwrap_or("pet")
                    .to_string(),
                importance: fact
                    .get("importance")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0),
                confidence: fact
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.7)
                    .clamp(0.0, 1.0),
            })
        })
        .collect())
}

fn memory_from_operation(operation: &MemoryOperation, id: Option<String>) -> MemoryFact {
    let now = now_ms();
    MemoryFact {
        id: id.unwrap_or_else(|| {
            format!(
                "memory-{now}-{}",
                REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            )
        }),
        content: operation.content.clone(),
        kind: operation.kind.clone(),
        scope: if operation.scope == "shared" {
            "shared".to_string()
        } else {
            "pet".to_string()
        },
        importance: operation.importance,
        confidence: operation.confidence,
        created_at: now,
        updated_at: now,
        status: "active".to_string(),
    }
}

fn forget_memory(
    app: &tauri::AppHandle,
    pet_id: &str,
    operation: &MemoryOperation,
) -> Result<(), String> {
    if operation.scope == "shared" {
        let mut facts = load_shared_memories(app);
        for fact in facts
            .iter_mut()
            .filter(|fact| fact.id == operation.target || fact.content == operation.target)
        {
            fact.status = "deleted".to_string();
            fact.updated_at = now_ms();
        }
        return write_shared_memories(app, &facts);
    }
    let matching_id = load_memories(app, pet_id)?
        .into_iter()
        .find(|fact| fact.id == operation.target || fact.content == operation.target)
        .map(|fact| fact.id)
        .unwrap_or_else(|| operation.target.clone());
    append_jsonl(
        memories_path(app, pet_id)?,
        &MemoryFact {
            id: matching_id,
            status: "deleted".to_string(),
            ..MemoryFact::default()
        },
    )
}

fn store_memory(app: &tauri::AppHandle, pet_id: &str, fact: &MemoryFact) -> Result<(), String> {
    if fact.content.trim().is_empty() {
        return Ok(());
    }
    if fact.scope == "shared" {
        return append_shared_memory(app, fact);
    }
    let path = memories_path(app, pet_id)?;
    let existing = load_memories(app, pet_id)?;
    if existing.iter().any(|memory| memory.content == fact.content) {
        return Ok(());
    }
    append_jsonl(path, fact)
}

async fn refresh_summary(
    app: tauri::AppHandle,
    pet_id: String,
    endpoint: ModelEndpointConfig,
    messages: Vec<ChatMessage>,
) {
    let keep_recent = 12usize;
    let older = messages.len().saturating_sub(keep_recent);
    if older < 1 {
        return;
    }
    let prompt = "你是桌面宠物的对话摘要器。把较早的聊天整理成一段简洁、客观、可供角色继续陪伴使用的中文摘要。保留用户明确表达的偏好、重要计划和共同经历，不编造信息，不写分析过程，不超过 1200 个中文字符。只返回摘要正文。";
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("summary client failed: {error}");
            return;
        }
    };
    let older_messages = messages.into_iter().take(older).collect::<Vec<_>>();
    match call_stream(
        &client,
        &endpoint,
        prompt,
        &older_messages,
        None,
        false,
        |_| {},
    )
    .await
    {
        Ok(summary) => {
            let summary = clean_reply(summary, 4_800);
            if !summary.is_empty() {
                if let Err(error) = save_summary(&app, &pet_id, &summary) {
                    eprintln!("summary save failed: {error}");
                }
            }
        }
        Err(error) => eprintln!("summary request failed: {error}"),
    }
}

async fn run_chat_task(
    app: tauri::AppHandle,
    pet_id: String,
    request_id: String,
    content: String,
) -> Result<(), String> {
    let config = config_snapshot(&app)?;
    let ai = config.ai.clone();
    if !ai.enabled {
        return Err("AI 对话未启用".to_string());
    }
    let endpoint = ai
        .chat_model
        .clone()
        .ok_or_else(|| "尚未配置聊天模型".to_string())?;
    let card = card_for_pet(&app, &pet_id)?;
    let mut messages = load_messages(&app, &pet_id)?;
    let user_message = ChatMessage {
        id: format!("message-{}", now_ms()),
        role: "user".to_string(),
        content: content.clone(),
        timestamp: now_ms(),
        source: "chat".to_string(),
        vision_summary: None,
    };
    append_message(&app, &pet_id, &user_message)?;
    messages.push(user_message);
    let memory = if ai.memory_enabled {
        choose_memories(&app, &pet_id, &content)
    } else {
        Vec::new()
    };
    let prompt = prompt_for(
        &card,
        &pet_id,
        &load_profile(&app),
        &memory,
        &load_summary(&app, &pet_id),
        &content,
    );
    let recent = history_for_prompt(&messages, ai.max_recent_messages);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let app_for_delta = app.clone();
    let request_for_delta = request_id.clone();
    let pet_for_delta = pet_id.clone();
    let reply = call_stream(
        &client,
        &endpoint,
        &prompt,
        &recent,
        None,
        true,
        move |delta| {
            let _ = app_for_delta.emit(
                "chat://delta",
                ChatDeltaEvent {
                    request_id: request_for_delta.clone(),
                    pet_id: pet_for_delta.clone(),
                    delta,
                },
            );
        },
    )
    .await?;
    let reply = clean_reply(reply, 600);
    if reply.is_empty() {
        return Err("模型没有返回文字".to_string());
    }
    let assistant = ChatMessage {
        id: format!("message-{}", now_ms()),
        role: "assistant".to_string(),
        content: reply,
        timestamp: now_ms(),
        source: "chat".to_string(),
        vision_summary: None,
    };
    append_message(&app, &pet_id, &assistant)?;
    let _ = app.emit(
        "chat://complete",
        ChatCompleteEvent {
            request_id,
            pet_id: pet_id.clone(),
            message: assistant.clone(),
        },
    );
    let memory_messages = {
        let mut messages = messages;
        messages.push(assistant.clone());
        messages
    };
    if ai.memory_enabled {
        let extraction_client = client.clone();
        let extraction_config = endpoint.clone();
        let extraction_messages = memory_messages.clone();
        let app_for_memory = app.clone();
        let pet_for_memory = pet_id.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(operations) =
                extract_memories(&extraction_client, &extraction_config, &extraction_messages).await
            {
                for operation in operations {
                    match operation.action.as_str() {
                        "forget" => {
                            let _ = forget_memory(&app_for_memory, &pet_for_memory, &operation);
                        }
                        "update" => {
                            let existing_id = if operation.scope == "shared" {
                                load_shared_memories(&app_for_memory)
                                    .into_iter()
                                    .find(|fact| {
                                        fact.id == operation.target
                                            || fact.content == operation.target
                                    })
                                    .map(|fact| fact.id)
                            } else {
                                load_memories(&app_for_memory, &pet_for_memory)
                                    .ok()
                                    .and_then(|facts| {
                                        facts
                                            .into_iter()
                                            .find(|fact| {
                                                fact.id == operation.target
                                                    || fact.content == operation.target
                                            })
                                            .map(|fact| fact.id)
                                    })
                            };
                            let fact = memory_from_operation(&operation, existing_id);
                            let _ = store_memory(&app_for_memory, &pet_for_memory, &fact);
                        }
                        _ => {
                            let fact = memory_from_operation(&operation, None);
                            let _ = store_memory(&app_for_memory, &pet_for_memory, &fact);
                        }
                    }
                }
            }
        });
    }
    if memory_messages.len() > 40 {
        let app_for_summary = app.clone();
        let pet_for_summary = pet_id.clone();
        let summary_endpoint = endpoint.clone();
        tauri::async_runtime::spawn(refresh_summary(
            app_for_summary,
            pet_for_summary,
            summary_endpoint,
            memory_messages,
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_ai_settings(app: tauri::AppHandle) -> Result<AiSettings, String> {
    Ok(config_snapshot(&app)?.ai)
}

#[tauri::command]
pub(crate) fn update_ai_settings(
    app: tauri::AppHandle,
    mut settings: AiSettings,
) -> Result<AiSettings, String> {
    settings.chat_model = settings.chat_model.map(normalize_endpoint_config);
    settings.vision_model = settings.vision_model.map(normalize_endpoint_config);
    settings.max_recent_messages = settings.max_recent_messages.clamp(2, 40);
    settings.heartbeat_min_minutes = settings.heartbeat_min_minutes.clamp(1, 1_440);
    settings.heartbeat_max_minutes = settings
        .heartbeat_max_minutes
        .max(settings.heartbeat_min_minutes)
        .clamp(1, 1_440);
    settings.heartbeat_vision_chance = settings.heartbeat_vision_chance.clamp(0.0, 1.0);
    let config = super::update_config(&app, |config| {
        config.ai = settings.clone();
        Ok(())
    })?;
    Ok(config.ai)
}

#[tauri::command]
pub(crate) fn set_ai_secret(reference: String, secret: String) -> Result<(), String> {
    if reference.len() > 100 || reference.is_empty() {
        return Err("密钥引用无效".to_string());
    }
    let entry = keyring::Entry::new(SERVICE_NAME, &reference).map_err(|error| error.to_string())?;
    entry
        .set_password(&secret)
        .map_err(|error| format!("保存 API Key 失败: {error}"))
}

#[tauri::command]
pub(crate) fn delete_ai_secret(reference: String) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, &reference).map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub(crate) async fn test_ai_provider(
    config: ModelEndpointConfig,
    vision: bool,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let prompt = if vision {
        "请回复 OK，表示你可以处理图片输入。"
    } else {
        "请只回复 OK。"
    };
    let mut messages = Vec::new();
    let image = if vision {
        Some(test_image_data_url()?)
    } else {
        None
    };
    if vision {
        messages.push(ChatMessage {
            id: "__vision__".to_string(),
            role: "user".to_string(),
            content: "测试图片".to_string(),
            timestamp: now_ms(),
            source: "test".to_string(),
            vision_summary: None,
        });
    }
    let result = call_stream(
        &client,
        &config,
        prompt,
        &messages,
        image.as_deref(),
        false,
        |_| {},
    )
    .await?;
    Ok(result.chars().take(80).collect())
}

fn test_image_data_url() -> Result<String, String> {
    let image = RgbaImage::from_pixel(2, 2, image::Rgba([157, 218, 228, 255]));
    let dynamic = DynamicImage::ImageRgba8(image);
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 70)
        .encode_image(&dynamic)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn capture_desktop(app: tauri::AppHandle) -> Result<String, String> {
    capture_desktop_data_url(&app).await
}

#[tauri::command]
pub(crate) fn get_chat_history(
    app: tauri::AppHandle,
    pet_id: String,
) -> Result<ChatHistoryResponse, String> {
    Ok(ChatHistoryResponse {
        pet_id: pet_id.clone(),
        messages: load_messages(&app, &pet_id)?
            .into_iter()
            .rev()
            .take(MAX_HISTORY_MESSAGES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
    })
}

#[tauri::command]
pub(crate) fn send_chat_message(
    app: tauri::AppHandle,
    pet_id: String,
    content: String,
) -> Result<ChatStarted, String> {
    let content = content.trim().to_string();
    if content.is_empty() || content.chars().count() > MAX_MESSAGE_CHARS {
        return Err("消息不能为空且不能超过 4000 字".to_string());
    }
    let request_id = request_id();
    let state = app.state::<AppState>();
    {
        let mut active_pets = state
            .ai
            .active_pets
            .lock()
            .map_err(|_| "AI 任务锁失败".to_string())?;
        if active_pets.contains_key(&pet_id) {
            return Err("这只宠物正在回复，请先停止当前回复".to_string());
        }
        active_pets.insert(pet_id.clone(), request_id.clone());
        let handle = tauri::async_runtime::spawn({
            let app = app.clone();
            let pet_id = pet_id.clone();
            let request_id = request_id.clone();
            async move {
                let result =
                    run_chat_task(app.clone(), pet_id.clone(), request_id.clone(), content).await;
                if let Err(message) = result {
                    let _ = app.emit(
                        "chat://error",
                        ChatErrorEvent {
                            request_id: request_id.clone(),
                            pet_id: pet_id.clone(),
                            message,
                        },
                    );
                }
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut active_pets) = state.ai.active_pets.lock() {
                        if active_pets
                            .get(&pet_id)
                            .is_some_and(|active_id| active_id == &request_id)
                        {
                            active_pets.remove(&pet_id);
                        }
                    }
                    if let Ok(mut tasks) = state.ai.tasks.lock() {
                        if tasks
                            .get(&pet_id)
                            .is_some_and(|(task_id, _)| task_id == &request_id)
                        {
                            tasks.remove(&pet_id);
                        }
                    }
                }
            }
        });
        match state.ai.tasks.lock() {
            Ok(mut tasks) => {
                tasks.insert(pet_id, (request_id.clone(), handle));
            }
            Err(_) => {
                active_pets.remove(&pet_id);
                handle.abort();
                return Err("AI 任务锁失败".to_string());
            }
        }
    }
    Ok(ChatStarted { request_id })
}

#[tauri::command]
pub(crate) fn cancel_chat_response(app: tauri::AppHandle, pet_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let task = state
        .ai
        .tasks
        .lock()
        .map_err(|_| "AI 任务锁失败".to_string())?
        .remove(&pet_id);
    if let Some((request_id, handle)) = task {
        handle.abort();
        if let Ok(mut active_pets) = state.ai.active_pets.lock() {
            if active_pets
                .get(&pet_id)
                .is_some_and(|active_id| active_id == &request_id)
            {
                active_pets.remove(&pet_id);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn clear_chat_history(app: tauri::AppHandle, pet_id: String) -> Result<(), String> {
    let path = messages_path(&app, &pet_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_memories(
    app: tauri::AppHandle,
    pet_id: String,
) -> Result<Vec<MemoryFact>, String> {
    load_memories(&app, &pet_id)
}

#[tauri::command]
pub(crate) fn delete_memory(
    app: tauri::AppHandle,
    pet_id: String,
    memory_id: String,
) -> Result<(), String> {
    let fact = MemoryFact {
        id: memory_id,
        status: "deleted".to_string(),
        ..MemoryFact::default()
    };
    append_jsonl(memories_path(&app, &pet_id)?, &fact)
}

#[tauri::command]
pub(crate) fn update_memory(
    app: tauri::AppHandle,
    pet_id: String,
    mut memory: MemoryFact,
) -> Result<(), String> {
    if memory.id.is_empty()
        || memory.id.len() > 100
        || memory.content.trim().is_empty()
        || memory.content.chars().count() > 300
    {
        return Err("记忆内容不能为空且不能超过 300 个字符".to_string());
    }
    if memory.scope == "shared" {
        return update_shared_memory(app, memory);
    }
    memory.scope = "pet".to_string();
    memory.status = "active".to_string();
    memory.content = memory.content.trim().to_string();
    memory.updated_at = now_ms();
    append_jsonl(memories_path(&app, &pet_id)?, &memory)
}

#[tauri::command]
pub(crate) fn clear_memories(app: tauri::AppHandle, pet_id: String) -> Result<(), String> {
    let path = memories_path(&app, &pet_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_shared_memories(app: tauri::AppHandle) -> Result<Vec<MemoryFact>, String> {
    Ok(load_shared_memories(&app)
        .into_iter()
        .filter(|fact| fact.status == "active")
        .collect())
}

#[tauri::command]
pub(crate) fn delete_shared_memory(app: tauri::AppHandle, memory_id: String) -> Result<(), String> {
    let mut facts = load_shared_memories(&app);
    for fact in facts.iter_mut().filter(|fact| fact.id == memory_id) {
        fact.status = "deleted".to_string();
        fact.updated_at = now_ms();
    }
    write_shared_memories(&app, &facts)
}

#[tauri::command]
pub(crate) fn update_shared_memory(
    app: tauri::AppHandle,
    mut memory: MemoryFact,
) -> Result<(), String> {
    if memory.id.is_empty()
        || memory.content.trim().is_empty()
        || memory.content.chars().count() > 300
    {
        return Err("记忆内容不能为空且不能超过 300 个字符".to_string());
    }
    memory.scope = "shared".to_string();
    memory.status = "active".to_string();
    memory.content = memory.content.trim().to_string();
    memory.updated_at = now_ms();
    let mut facts = load_shared_memories(&app);
    if let Some(existing) = facts.iter_mut().find(|fact| fact.id == memory.id) {
        *existing = memory;
    } else {
        facts.push(memory);
    }
    write_shared_memories(&app, &facts)
}

#[tauri::command]
pub(crate) fn clear_shared_memories(app: tauri::AppHandle) -> Result<(), String> {
    let path = profile_path(&app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn random_heartbeat_delay(settings: &AiSettings) -> Duration {
    let mut rng = rand::rng();
    let min = settings
        .heartbeat_min_minutes
        .min(settings.heartbeat_max_minutes);
    let max = settings.heartbeat_max_minutes.max(min);
    Duration::from_secs(rng.random_range(min..=max) as u64 * 60)
}

pub(crate) fn settings_have_chat(settings: &AiSettings) -> bool {
    settings.enabled
        && settings
            .chat_model
            .as_ref()
            .is_some_and(|model| !model.model.trim().is_empty())
}

async fn run_heartbeat(app: tauri::AppHandle, pet_id: String) -> Result<(), String> {
    let config = config_snapshot(&app)?;
    let ai = config.ai.clone();
    if !settings_have_chat(&ai) || !ai.heartbeat_enabled {
        return Ok(());
    }
    let pet_settings = super::settings_for_pet(&config, &pet_id);
    if pet_settings.paused || pet_settings.quiet_mode {
        return Ok(());
    }
    let endpoint = ai
        .chat_model
        .clone()
        .ok_or_else(|| "尚未配置聊天模型".to_string())?;
    let card = card_for_pet(&app, &pet_id)?;
    let messages = load_messages(&app, &pet_id)?;
    if messages
        .last()
        .is_some_and(|message| now_ms().saturating_sub(message.timestamp) < 5 * 60 * 1000)
    {
        return Ok(());
    }
    let memory = if ai.memory_enabled {
        choose_memories(&app, &pet_id, "最近的陪伴和用户")
    } else {
        Vec::new()
    };
    let mut prompt = prompt_for(
        &card,
        &pet_id,
        &load_profile(&app),
        &memory,
        &load_summary(&app, &pet_id),
        "heartbeat",
    );
    prompt.push_str("\n\n这是一次安静的 heartbeat。只有在确实有自然、和当前关系有关的话可说时才回复；否则只返回 NO_REPLY。回复最多 80 个中文字符，不要提及你是模型。");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let mut vision_summary = None;
    let can_use_vision_model = ai.desktop_vision_enabled
        && ai
            .vision_model
            .as_ref()
            .is_some_and(|model| !model.model.trim().is_empty());
    let chat_window_open = app.webview_windows().iter().any(|(label, window)| {
        label.starts_with("pet-chat-") && window.is_visible().unwrap_or(false)
    });
    let should_use_vision = {
        let mut rng = rand::rng();
        rng.random::<f64>() < ai.heartbeat_vision_chance
    };
    let vision_due =
        now_ms().saturating_sub(LAST_VISION_MS.load(Ordering::Relaxed)) >= 60 * 60 * 1000;
    if can_use_vision_model && !chat_window_open && vision_due && should_use_vision {
        // Mark the hour before capturing so a denied permission cannot cause a
        // rapid retry loop. A failed capture simply falls back to normal chat.
        LAST_VISION_MS.store(now_ms(), Ordering::Relaxed);
        if let Ok(image) = capture_desktop_data_url(&app).await {
            let vision_endpoint = ai.vision_model.clone().expect("checked above");
            let vision_message = ChatMessage {
                id: "__vision__".to_string(),
                role: "user".to_string(),
                content: "请观察这张桌面截图，只返回事实性的中文摘要；不要复述密码、令牌、私人联系方式或其他敏感文本。".to_string(),
                timestamp: now_ms(),
                source: "vision".to_string(),
                vision_summary: None,
            };
            if let Ok(summary) = call_stream(&client, &vision_endpoint, "你是一个严格的桌面视觉观察器。只描述看得见的非敏感事实，不进行推断，不输出角色台词。", &[vision_message], Some(&image), false, |_| {}).await {
                let summary = clean_reply(summary, 600);
                if !summary.is_empty() {
                    vision_summary = Some(summary);
                    prompt.push_str("\n\n当前桌面观察（仅作为上下文，不要复述敏感信息）：\n");
                    prompt.push_str(vision_summary.as_deref().unwrap_or_default());
                }
            }
        }
    }
    let result = call_stream(
        &client,
        &endpoint,
        &prompt,
        &history_for_prompt(&messages, ai.max_recent_messages),
        None,
        false,
        |_| {},
    )
    .await?;
    let result = clean_reply(result, 80);
    if result.is_empty() || result.eq_ignore_ascii_case("NO_REPLY") {
        return Ok(());
    }
    let message = ChatMessage {
        id: format!("heartbeat-{}", now_ms()),
        role: "assistant".to_string(),
        content: result,
        timestamp: now_ms(),
        source: "heartbeat".to_string(),
        vision_summary,
    };
    append_message(&app, &pet_id, &message)?;
    let _ = app.emit(
        "chat://complete",
        ChatCompleteEvent {
            request_id: format!("heartbeat-{}", now_ms()),
            pet_id,
            message,
        },
    );
    Ok(())
}

pub(crate) fn start_heartbeat_scheduler(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut next_heartbeat: HashMap<String, u64> = HashMap::new();
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let Ok(config) = config_snapshot(&app) else {
                continue;
            };
            if !settings_have_chat(&config.ai) || !config.ai.heartbeat_enabled {
                continue;
            }
            let candidates: Vec<String> = super::visible_instances(&app, &config)
                .into_iter()
                .filter(|instance| {
                    let settings = super::settings_for_pet(&config, &instance.pet_id);
                    !settings.paused && !settings.quiet_mode
                })
                .map(|instance| instance.pet_id)
                .collect();
            let candidate_ids: HashSet<&str> = candidates.iter().map(String::as_str).collect();
            next_heartbeat.retain(|pet_id, _| candidate_ids.contains(pet_id.as_str()));
            let now = now_ms();
            for pet_id in &candidates {
                next_heartbeat.entry(pet_id.clone()).or_insert_with(|| {
                    now.saturating_add(random_heartbeat_delay(&config.ai).as_millis() as u64)
                });
            }
            if now.saturating_sub(LAST_HEARTBEAT_MS.load(Ordering::Relaxed)) < 10 * 60 * 1000 {
                continue;
            }
            let Some(pet_id) = candidates
                .into_iter()
                .find(|pet_id| next_heartbeat.get(pet_id).is_some_and(|due| *due <= now))
            else {
                continue;
            };
            if app
                .state::<AppState>()
                .ai
                .active_pets
                .lock()
                .map(|pets| !pets.is_empty())
                .unwrap_or(true)
            {
                continue;
            }
            LAST_HEARTBEAT_MS.store(now, Ordering::Relaxed);
            let due_after =
                now.saturating_add(random_heartbeat_delay(&config.ai).as_millis() as u64);
            next_heartbeat.insert(pet_id.clone(), due_after);
            let active_inserted = app
                .state::<AppState>()
                .ai
                .active_pets
                .lock()
                .map(|mut pets| {
                    pets.insert(pet_id.clone(), format!("heartbeat-{now}"))
                        .is_none()
                })
                .unwrap_or(false);
            if !active_inserted {
                continue;
            }
            if let Err(error) = run_heartbeat(app.clone(), pet_id.clone()).await {
                eprintln!("heartbeat failed: {error}");
            }
            if let Ok(mut pets) = app.state::<AppState>().ai.active_pets.lock() {
                if pets
                    .get(&pet_id)
                    .is_some_and(|active_id| active_id == &format!("heartbeat-{now}"))
                {
                    pets.remove(&pet_id);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(content: &str, importance: f64) -> MemoryFact {
        MemoryFact {
            content: content.to_string(),
            importance,
            confidence: 1.0,
            updated_at: now_ms(),
            ..MemoryFact::default()
        }
    }

    #[test]
    fn chinese_memory_search_prefers_matching_fact() {
        let results = relevant_memories(
            &[memory("用户喜欢粉蓝色", 0.5), memory("用户喜欢猫粮", 0.5)],
            "粉蓝色界面",
        );
        assert_eq!(
            results.first().map(|fact| fact.content.as_str()),
            Some("用户喜欢粉蓝色")
        );
    }

    #[test]
    fn heartbeat_delay_stays_within_configured_range() {
        let settings = AiSettings {
            heartbeat_min_minutes: 20,
            heartbeat_max_minutes: 60,
            ..AiSettings::default()
        };
        for _ in 0..20 {
            let seconds = random_heartbeat_delay(&settings).as_secs();
            assert!((20 * 60..=60 * 60).contains(&seconds));
        }
    }

    #[test]
    fn sse_boundary_accepts_lf_and_crlf() {
        assert_eq!(sse_boundary("data: {}\n\nrest"), Some((8, 2)));
        assert_eq!(sse_boundary("data: {}\r\n\r\nrest"), Some((8, 4)));
    }

    #[test]
    fn provider_stream_deltas_are_normalized() {
        assert_eq!(
            stream_delta(
                &json!({"type":"response.output_text.delta","delta":"你好"}),
                &ProviderKind::OpenaiResponses
            ),
            Some("你好".to_string())
        );
        assert_eq!(
            stream_delta(
                &json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"你好"}}),
                &ProviderKind::AnthropicMessages
            ),
            Some("你好".to_string())
        );
        assert_eq!(
            stream_delta(
                &json!({"choices":[{"delta":{"content":"你好"}}]}),
                &ProviderKind::OpenaiCompatible
            ),
            Some("你好".to_string())
        );
    }

    #[test]
    fn prompt_puts_app_safety_before_character_card_and_excludes_creator_notes() {
        let card = CharacterCard {
            system_prompt: "角色提示".to_string(),
            creator_notes: "不应该发给模型".to_string(),
            ..CharacterCard::default()
        };
        let prompt = prompt_for(&card, "saki", "", &[], "", "");
        assert!(prompt.find("只能进行聊天").unwrap() < prompt.find("角色提示").unwrap());
        assert!(!prompt.contains("不应该发给模型"));
    }

    #[test]
    fn provider_payloads_keep_protocol_specific_fields() {
        let messages = vec![ChatMessage {
            id: "message".to_string(),
            role: "user".to_string(),
            content: "你好".to_string(),
            timestamp: now_ms(),
            source: "chat".to_string(),
            vision_summary: None,
        }];
        let responses = ModelEndpointConfig {
            model: "gpt-test".to_string(),
            ..ModelEndpointConfig::default()
        };
        let responses_payload = build_payload(&responses, "system", &messages, None, true);
        assert_eq!(responses_payload["instructions"], "system");
        assert_eq!(responses_payload["store"], false);
        assert_eq!(responses_payload["stream"], true);

        let anthropic = ModelEndpointConfig {
            provider: ProviderKind::AnthropicMessages,
            model: "claude-test".to_string(),
            ..ModelEndpointConfig::default()
        };
        let anthropic_payload = build_payload(&anthropic, "system", &messages, None, false);
        assert_eq!(anthropic_payload["system"], "system");
        assert_eq!(anthropic_payload["max_tokens"], 300);
        assert_eq!(anthropic_payload["stream"], false);

        let compatible = ModelEndpointConfig {
            provider: ProviderKind::OpenaiCompatible,
            model: "local-test".to_string(),
            ..ModelEndpointConfig::default()
        };
        let compatible_payload = build_payload(&compatible, "system", &messages, None, true);
        assert_eq!(compatible_payload["messages"][0]["role"], "system");
        assert_eq!(compatible_payload["messages"][1]["content"], "你好");
    }

    #[test]
    fn vision_payload_contains_image_without_persisting_it() {
        let config = ModelEndpointConfig {
            provider: ProviderKind::OpenaiCompatible,
            model: "local-vision".to_string(),
            ..ModelEndpointConfig::default()
        };
        let payload = build_payload(
            &config,
            "vision",
            &[ChatMessage {
                id: "__vision__".to_string(),
                role: "user".to_string(),
                content: "观察".to_string(),
                timestamp: now_ms(),
                source: "vision".to_string(),
                vision_summary: None,
            }],
            Some("data:image/jpeg;base64,abc"),
            false,
        );
        assert_eq!(
            payload["messages"][1]["content"][1]["image_url"]["url"],
            "data:image/jpeg;base64,abc"
        );
    }
}
