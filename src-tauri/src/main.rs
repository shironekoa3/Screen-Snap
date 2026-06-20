#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod screenshot;

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use base64::Engine;
use tauri::Emitter;

// ── Settings ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Settings {
    interval_secs: u64,
    format: String,
    quality: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            format: "png".to_string(),
            quality: 80,
        }
    }
}

// ── Event Payloads ────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
struct ScreenshotPayload {
    filepath: String,
    base64: String,
}

#[derive(Debug, Serialize, Clone)]
struct CaptureResult {
    filepath: String,
    base64: String,
}

// ── App State ─────────────────────────────────────────────────

struct AppState {
    capture_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    capture_running: std::sync::Arc<AtomicBool>,
}

impl AppState {
    fn new() -> Self {
        Self {
            capture_handle: Mutex::new(None),
            capture_running: std::sync::Arc::new(AtomicBool::new(false)),
        }
    }
}

// ── Paths ─────────────────────────────────────────────────────

fn app_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf()
}

fn screenshots_dir() -> PathBuf {
    let dir = app_dir().join("screenshots");
    fs::create_dir_all(&dir).ok();
    dir
}

fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

// ── Core Capture Logic ────────────────────────────────────────

fn do_capture(format: &str, quality: u8) -> Result<CaptureResult, String> {
    let base = screenshots_dir();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let date_dir = base.join(&today);
    fs::create_dir_all(&date_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let timestamp = Local::now().format("%H-%M-%S").to_string();
    let ext = match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        _ => "png",
    };
    let filename = format!("{}.{}", timestamp, ext);
    let filepath = date_dir.join(&filename);

    let (data, width, height) = screenshot::capture_screen()?;
    screenshot::save_screenshot(&data, width, height, &filepath, format, quality)?;

    let path_str = filepath.to_string_lossy().to_string();
    let file_data =
        fs::read(&filepath).map_err(|e| format!("读取截图失败: {}", e))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&file_data);

    Ok(CaptureResult {
        filepath: path_str,
        base64: b64,
    })
}

// ── Tauri Commands ────────────────────────────────────────────

#[tauri::command]
async fn take_screenshot(format: String, quality: u8) -> Result<CaptureResult, String> {
    do_capture(&format, quality)
}

#[tauri::command]
async fn read_screenshot_data(filepath: String) -> Result<String, String> {
    let data = fs::read(&filepath).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

#[tauri::command]
async fn start_capture(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    interval_secs: u64,
    format: String,
    quality: u8,
) -> Result<(), String> {
    let running = state.capture_running.clone();

    // Check if already running
    if running.load(Ordering::SeqCst) {
        return Err("截图任务已在运行中".to_string());
    }

    running.store(true, Ordering::SeqCst);

    let r = running.clone();
    let fmt = format.clone();
    let q = quality;
    let app_handle = app.clone();

    let handle = tokio::spawn(async move {
        while r.load(Ordering::SeqCst) {
            match do_capture(&fmt, q) {
                Ok(result) => {
                    let _ = app_handle.emit("screenshot-taken", ScreenshotPayload {
                        filepath: result.filepath,
                        base64: result.base64,
                    });
                }
                Err(e) => {
                    let _ = app_handle.emit("screenshot-error", e);
                }
            }

            // Sleep with cancellation check every 200ms
            let total_sleep_ms = interval_secs * 1000;
            let mut elapsed = 0u64;
            while elapsed < total_sleep_ms {
                if !r.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                elapsed += 200;
            }
        }
    });

    *state.capture_handle.lock().unwrap() = Some(handle);

    Ok(())
}

#[tauri::command]
async fn stop_capture(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.capture_running.store(false, Ordering::SeqCst);

    if let Some(handle) = state.capture_handle.lock().unwrap().take() {
        handle.abort();
    }

    Ok(())
}

#[tauri::command]
async fn get_capture_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.capture_running.load(Ordering::SeqCst))
}

#[tauri::command]
async fn get_screenshots_by_date(date: String) -> Result<Vec<String>, String> {
    let dir = screenshots_dir().join(&date);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut files: Vec<String> = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    // Sort by filename descending (newest first)
    files.sort_by(|a, b| b.cmp(a));
    Ok(files)
}

#[tauri::command]
async fn get_dates_with_screenshots() -> Result<Vec<String>, String> {
    let dir = screenshots_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut dates: Vec<String> = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Validate date format YYYY-MM-DD
            if name.len() == 10
                && name.chars().nth(4) == Some('-')
                && name.chars().nth(7) == Some('-')
            {
                dates.push(name);
            }
        }
    }

    // Sort descending
    dates.sort_by(|a, b| b.cmp(a));
    Ok(dates)
}

#[tauri::command]
async fn delete_screenshot(filepath: String) -> Result<(), String> {
    fs::remove_file(&filepath).map_err(|e| format!("删除文件失败: {}", e))
}

#[tauri::command]
async fn get_settings() -> Result<Settings, String> {
    let path = settings_path();
    if path.exists() {
        let content =
            fs::read_to_string(&path).map_err(|e| format!("读取设置文件失败: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("解析设置失败: {}", e))
    } else {
        Ok(Settings::default())
    }
}

#[tauri::command]
async fn save_settings(settings: Settings) -> Result<(), String> {
    let path = settings_path();
    let content =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化设置失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("保存设置失败: {}", e))
}

#[tauri::command]
async fn get_storage_info() -> Result<serde_json::Value, String> {
    let dir = screenshots_dir();
    let (total_size, total_count) = calculate_dir_size(&dir);

    let today = Local::now().format("%Y-%m-%d").to_string();
    let today_dir = dir.join(&today);
    let (today_size, today_count) = if today_dir.exists() {
        calculate_dir_size(&today_dir)
    } else {
        (0, 0)
    };

    Ok(serde_json::json!({
        "total_size_bytes": total_size,
        "total_count": total_count,
        "today_size_bytes": today_size,
        "today_count": today_count,
    }))
}

fn calculate_dir_size(dir: &PathBuf) -> (u64, u64) {
    let mut total_size: u64 = 0;
    let mut total_count: u64 = 0;
    calc_recursive(dir, &mut total_size, &mut total_count);
    (total_size, total_count)
}

fn calc_recursive(dir: &PathBuf, size: &mut u64, count: &mut u64) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    *size += meta.len();
                    *count += 1;
                }
            } else if path.is_dir() {
                calc_recursive(&path, size, count);
            }
        }
    }
}

// ── Main ──────────────────────────────────────────────────────

fn main() {
    let state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            take_screenshot,
            read_screenshot_data,
            start_capture,
            stop_capture,
            get_capture_status,
            get_screenshots_by_date,
            get_dates_with_screenshots,
            delete_screenshot,
            get_settings,
            save_settings,
            get_storage_info,
        ])
        .run(tauri::generate_context!())
        .expect("启动应用失败");
}
