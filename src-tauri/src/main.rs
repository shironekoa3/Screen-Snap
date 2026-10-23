#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod ocr;
mod screenshot;

use base64::Engine;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tokio::sync::Semaphore;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// 缩略图最长边，用于历史/搜索列表与预览，避免整图 base64 走 IPC
const THUMB_MAX_WIDTH: u32 = 640;
const THUMB_QUALITY: u8 = 75;
/// OCR 串行执行，避免定时截图密集时堆积线程与 CPU
const OCR_CONCURRENCY: usize = 1;
/// 缩略图目录（位于 screenshots/ 下，以点开头，不会被当成日期目录）
const THUMBS_DIR_NAME: &str = ".thumbs";

/// 「启动时自动开始记录」启动后先等这么久再截第一张，给桌面/显卡驱动留出就绪时间
const AUTO_START_DELAY_MS: u64 = 3000;

// ── Settings ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    interval_secs: u64,
    format: String,
    quality: u8,
    /// 关闭窗口时是收进托盘（true）还是直接退出（false）
    close_to_tray: bool,
    /// 程序启动时自动开始定时截图
    auto_start_capture: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: 10,
            format: "jpg".to_string(),
            quality: 80,
            close_to_tray: true,
            auto_start_capture: false,
        }
    }
}

// ── Event Payloads ────────────────────────────────────────────

/// 截图结果。`thumbnail` 是缩略图的 base64（几十 KB），不是原图。
/// `pending` 为 true 表示原图还在后台编码落盘，此时按 filepath 读文件可能还读不到。
#[derive(Debug, Serialize, Clone)]
struct CaptureResult {
    filepath: String,
    thumbnail: String,
    pending: bool,
}

/// 已截好、待落盘的一帧。原图编码是整条链路里最慢的一步，放到后台做。
struct PendingShot {
    filepath: PathBuf,
    image: image::RgbaImage,
    format: String,
    quality: u8,
    /// 截屏时顺手生成的缩略图字节，后台只需写盘，不用再算一遍
    thumbnail: Vec<u8>,
}

#[derive(Debug, Serialize, Clone)]
struct SearchResult {
    filepath: String,
    ocr_text: String,
    captured_at: String,
}

// ── App State ─────────────────────────────────────────────────

struct AppState {
    capture_handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    capture_running: Arc<AtomicBool>,
    db: Arc<db::Database>,
    ocr_slots: Arc<Semaphore>,
    /// 关闭窗口的行为，随设置实时更新（窗口事件里读不到 tauri::State，所以单独存一份）
    close_to_tray: Arc<AtomicBool>,
    /// 上一张「已保存」截图的指纹。新的一帧和它足够像就直接跳过，不写盘也不 OCR。
    /// 手动截图和定时截图共用，保证两边的判断基准一致。
    last_fingerprint: Arc<Mutex<Option<Vec<u8>>>>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        let dir = app_dir();
        fs::create_dir_all(&dir)
            .map_err(|e| format!("无法创建数据目录 {}: {}", dir.display(), e))?;

        let db_path = dir.join("ocr.db");
        let database = db::Database::new(&db_path)?;
        let close_to_tray = read_settings().close_to_tray;

        Ok(Self {
            capture_handle: Mutex::new(None),
            capture_running: Arc::new(AtomicBool::new(false)),
            db: Arc::new(database),
            ocr_slots: Arc::new(Semaphore::new(OCR_CONCURRENCY)),
            close_to_tray: Arc::new(AtomicBool::new(close_to_tray)),
            last_fingerprint: Arc::new(Mutex::new(None)),
        })
    }
}

/// 后台任务：编码原图 → 落盘 → 写缩略图 → OCR。
/// 截屏本身只要几十毫秒，整条链路里最慢的是原图编码，所以全部挪到这里。
/// OCR 用信号量串行，避免短间隔下堆积线程。
fn spawn_save_task(db: Arc<db::Database>, slots: Arc<Semaphore>, shot: PendingShot) {
    tokio::spawn(async move {
        let saved = tokio::task::spawn_blocking(move || persist_shot(shot)).await;

        let filepath = match saved {
            Ok(Ok(fp)) => fp,
            Ok(Err(e)) => {
                eprintln!("保存截图失败: {}", e);
                return;
            }
            Err(e) => {
                eprintln!("保存任务异常: {}", e);
                return;
            }
        };

        let _permit = slots.acquire().await.ok();

        let fp = filepath.clone();
        let text = tokio::task::spawn_blocking(move || ocr::recognize_text_blocking(&fp)).await;
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        match text {
            Ok(Ok(out)) => {
                let _ = db.insert_or_update_ocr(&filepath, &out.flat, &out.display, &ts);
            }
            Ok(Err(e)) => {
                eprintln!("OCR 识别失败 [{}]: {}", filepath, e);
                // Store empty text so we don't retry this file
                let _ = db.insert_or_update_ocr(&filepath, "", "", &ts);
            }
            Err(e) => {
                eprintln!("OCR 任务异常 [{}]: {}", filepath, e);
            }
        }
    });
}

// ── Autostart ─────────────────────────────────────────────────

/// 当前用户的开机启动项（不需要管理员权限）
const AUTOSTART_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const AUTOSTART_VALUE: &str = "ScreenSnap";
/// 开机启动时带这个参数：只进托盘，不弹出主窗口
const AUTOSTART_ARG: &str = "--autostart";

fn autostart_command() -> String {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("screen-snapshot.exe"));
    format!("\"{}\" {}", exe.display(), AUTOSTART_ARG)
}

fn autostart_key() -> std::io::Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(AUTOSTART_SUBKEY)
}

fn is_autostart_enabled() -> bool {
    autostart_key()
        .and_then(|key| key.get_value::<String, _>(AUTOSTART_VALUE))
        .is_ok()
}

fn apply_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(AUTOSTART_SUBKEY)
            .map_err(|e| format!("打开开机启动项失败: {}", e))?;
        key.set_value(AUTOSTART_VALUE, &autostart_command())
            .map_err(|e| format!("写入开机启动项失败: {}", e))
    } else {
        // 注意：必须用可写句柄。open_subkey 是只读的，拿它 delete_value 会报
        // ERROR_ACCESS_DENIED (os error 5)。create_subkey 会以写权限打开（不存在则创建）。
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(AUTOSTART_SUBKEY)
            .map_err(|e| format!("打开开机启动项失败: {}", e))?
            .0;

        // 本来就没有启动项时不算失败
        match key.delete_value(AUTOSTART_VALUE) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("删除开机启动项失败: {}", e)),
        }
    }
}

// ── Paths ─────────────────────────────────────────────────────

/// 便携模式：数据与程序放在同一目录（用户明确要求，不要改成 AppData）。
fn app_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

fn screenshots_dir() -> PathBuf {
    let dir = app_dir().join("screenshots");
    fs::create_dir_all(&dir).ok();
    dir
}

fn thumbs_dir() -> PathBuf {
    let dir = screenshots_dir().join(THUMBS_DIR_NAME);
    fs::create_dir_all(&dir).ok();
    dir
}

fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

/// 读取设置文件；文件缺失或损坏时回落到默认值。
fn read_settings() -> Settings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// 缩略图与截图一一对应：screenshots/.thumbs/YYYY-MM-DD/HH-MM-SS-fff.jpg
fn thumb_path_for(image_path: &Path) -> PathBuf {
    let date = image_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = image_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    thumbs_dir().join(date).join(format!("{}.jpg", stem))
}

/// 只允许访问 screenshots/ 之下的文件，避免前端传入任意路径读写磁盘。
fn ensure_in_screenshots(filepath: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(screenshots_dir())
        .map_err(|e| format!("无法定位截图目录: {}", e))?;
    let target = fs::canonicalize(filepath).map_err(|_| format!("文件不存在: {}", filepath))?;
    if !target.starts_with(&root) {
        return Err("非法的文件路径".to_string());
    }
    Ok(target)
}

// ── Core Capture Logic ────────────────────────────────────────

/// 本次截图的落盘路径：screenshots/YYYY-MM-DD/HH-MM-SS-fff.ext
fn new_shot_path(format: &str) -> Result<PathBuf, String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let date_dir = screenshots_dir().join(&today);
    fs::create_dir_all(&date_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    // 毫秒级时间戳，避免同一秒内的手动截图与定时截图互相覆盖
    let timestamp = Local::now().format("%H-%M-%S-%3f").to_string();
    let ext = match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        _ => "png",
    };
    Ok(date_dir.join(format!("{}.{}", timestamp, ext)))
}

/// 快速路径：只做「截屏 + 生成缩略图」，几十毫秒即可返回，用来立刻给前端出图。
/// 原图编码（PNG 最慢，2560×1600 要几百毫秒）交给 `persist_shot` 在后台做。
/// 判定「和上一张一样」的阈值：64×36 灰度图的平均像素差。
/// 实测同一屏（仅任务栏时钟变化）MAD 在 0~2 之间，切页面时普遍 > 5。
const DUPLICATE_MAD_THRESHOLD: f64 = 2.0;

fn capture_quick(
    format: &str,
    quality: u8,
    last_fp: &Arc<Mutex<Option<Vec<u8>>>>,
) -> Result<Option<(CaptureResult, PendingShot)>, String> {
    let (data, width, height) = screenshot::capture_screen()?;
    // 从原始字节直接接管缓冲区，不再复制一份 15 MB
    let image = image::RgbaImage::from_raw(width, height, data)
        .ok_or_else(|| "无法创建图像缓冲区".to_string())?;

    // 先比指纹再决定要不要建文件：和上一张够像就直接返回 None，不产生任何磁盘写入
    let fp = screenshot::fingerprint(&image);
    {
        let mut last = last_fp.lock().map_err(|e| format!("状态锁失败: {}", e))?;
        if let Some(prev) = last.as_ref() {
            if screenshot::fingerprint_diff(prev, &fp) < DUPLICATE_MAD_THRESHOLD {
                return Ok(None);
            }
        }
        *last = Some(fp);
    }

    let filepath = new_shot_path(format)?;
    let thumbnail = screenshot::encode_thumbnail(&image, THUMB_MAX_WIDTH, THUMB_QUALITY)?;

    let result = CaptureResult {
        filepath: filepath.to_string_lossy().to_string(),
        thumbnail: base64::engine::general_purpose::STANDARD.encode(&thumbnail),
        pending: true,
    };

    let shot = PendingShot {
        filepath,
        image,
        format: format.to_string(),
        quality,
        thumbnail,
    };

    Ok(Some((result, shot)))
}

/// 后台路径：编码原图、写盘、写缩略图。返回最终文件路径。
fn persist_shot(shot: PendingShot) -> Result<String, String> {
    let encoded = screenshot::encode_image(&shot.image, &shot.format, shot.quality)?;
    fs::write(&shot.filepath, &encoded).map_err(|e| format!("写入截图失败: {}", e))?;

    // 缩略图在截屏时已经算好，这里只写盘
    let thumb_path = thumb_path_for(&shot.filepath);
    if let Some(parent) = thumb_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建缩略图目录失败: {}", e))?;
    }
    fs::write(&thumb_path, &shot.thumbnail).map_err(|e| format!("写入缩略图失败: {}", e))?;

    Ok(shot.filepath.to_string_lossy().to_string())
}

// ── Tauri Commands ────────────────────────────────────────────

#[tauri::command]
async fn take_screenshot(
    format: String,
    quality: u8,
    state: tauri::State<'_, AppState>,
) -> Result<Option<CaptureResult>, String> {
    let fmt = format.clone();
    let fp = state.last_fingerprint.clone();
    // 只等截屏 + 缩略图，几百毫秒的编码落盘不阻塞这里
    let captured = tokio::task::spawn_blocking(move || capture_quick(&fmt, quality, &fp))
        .await
        .map_err(|e| format!("截图任务异常: {}", e))??;

    // None 表示画面和上一张一样，这次不保存
    let Some((result, shot)) = captured else {
        return Ok(None);
    };

    spawn_save_task(state.db.clone(), state.ocr_slots.clone(), shot);
    Ok(Some(result))
}

/// 读取原图（灯箱预览用）。体积较大，列表不要用它。
#[tauri::command]
async fn read_screenshot_data(filepath: String) -> Result<String, String> {
    let path = ensure_in_screenshots(&filepath)?;
    let data = fs::read(&path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

/// 读取缩略图（列表/预览用）。缩略图缺失时按需生成并缓存到磁盘。
#[tauri::command]
async fn read_thumbnail_data(filepath: String) -> Result<String, String> {
    let path = ensure_in_screenshots(&filepath)?;
    let thumb = thumb_path_for(&path);

    let bytes = if thumb.exists() {
        fs::read(&thumb).map_err(|e| format!("读取缩略图失败: {}", e))?
    } else {
        tokio::task::spawn_blocking(move || {
            let img = screenshot::load_rgba(&path)?;
            screenshot::make_thumbnail(&img, &thumb, THUMB_MAX_WIDTH, THUMB_QUALITY)
        })
        .await
        .map_err(|e| format!("缩略图任务异常: {}", e))??
    };

    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 用系统默认程序打开截图（控制台双击预览时调用）。
#[tauri::command]
async fn open_screenshot(filepath: String) -> Result<(), String> {
    // 校验路径必须在 screenshots/ 之下（顺带确认文件已经落盘）
    ensure_in_screenshots(&filepath).map_err(|_| "图片还没保存完成，请稍后再试".to_string())?;

    // 交给资源管理器按文件关联打开。
    // 注意用原始路径，canonicalize 之后的 \\?\ 前缀 explorer 不认。
    std::process::Command::new("explorer")
        .arg(&filepath)
        .spawn()
        .map_err(|e| format!("打开图片失败: {}", e))?;

    Ok(())
}

/// 可取消的等待：每 200ms 检查一次停止标志，避免 stop 后还要干等一整个间隔。
async fn wait_cancellable(running: &Arc<AtomicBool>, total_ms: u64) {
    let mut elapsed = 0u64;
    while elapsed < total_ms {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        elapsed += 200;
    }
}

/// 启动定时截图循环。手动点击和「启动时自动开始记录」共用这一份实现。
/// `start_delay_ms`：第一次截图前的等待（自动启动时给系统留出就绪时间）。
fn start_capture_loop(
    app: &tauri::AppHandle,
    state: &AppState,
    interval_secs: u64,
    format: String,
    quality: u8,
    start_delay_ms: u64,
) -> Result<(), String> {
    let running = state.capture_running.clone();

    if running.load(Ordering::SeqCst) {
        return Err("截图任务已在运行中".to_string());
    }

    // 后端兜底校验，避免 0 秒导致空转死循环
    let interval_secs = interval_secs.clamp(1, 86_400);

    running.store(true, Ordering::SeqCst);

    let r = running.clone();
    let app_handle = app.clone();
    let db_clone = state.db.clone();
    let slots = state.ocr_slots.clone();
    let fp_state = state.last_fingerprint.clone();

    // 用 tauri 的运行时而不是 tokio::spawn：这个函数既会被命令调用，
    // 也会在 setup 里调用，而 setup 不在 Tokio runtime 上下文中，
    // 直接 tokio::spawn 会 panic（"there is no reactor running"）。
    let handle = tauri::async_runtime::spawn(async move {
        // 自动启动时先等几秒，等桌面/驱动就绪再截第一张
        if start_delay_ms > 0 {
            wait_cancellable(&r, start_delay_ms).await;
        }

        while r.load(Ordering::SeqCst) {
            let fmt = format.clone();
            let fp = fp_state.clone();
            // 只等截屏 + 缩略图；编码落盘和 OCR 都在后台任务里
            let captured =
                tokio::task::spawn_blocking(move || capture_quick(&fmt, quality, &fp)).await;

            match captured {
                Ok(Ok(Some((result, shot)))) => {
                    spawn_save_task(db_clone.clone(), slots.clone(), shot);
                    let _ = app_handle.emit("screenshot-taken", result);
                }
                // 和上一张几乎一样：静默跳过，不写盘、不 OCR、也不打扰界面
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    let _ = app_handle.emit("screenshot-error", e);
                }
                Err(e) => {
                    let _ = app_handle.emit("screenshot-error", format!("截图任务异常: {}", e));
                }
            }

            // Sleep with cancellation check every 200ms
            wait_cancellable(&r, interval_secs * 1000).await;
        }
    });

    *state.capture_handle.lock().unwrap() = Some(handle);

    Ok(())
}

/// 在资源管理器中打开截图所在目录，并选中这个文件。
#[tauri::command]
async fn reveal_screenshot(filepath: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    // 校验路径必须在 screenshots/ 之下
    ensure_in_screenshots(&filepath)?;

    // explorer 的 /select 参数不能带空格，必须拼成 `/select,"路径"`。
    // 用 raw_arg 避免 std 再套一层引号（否则 explorer 不认）。
    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", filepath))
        .spawn()
        .map_err(|e| format!("打开目录失败: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn start_capture(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    interval_secs: u64,
    format: String,
    quality: u8,
) -> Result<(), String> {
    start_capture_loop(&app, state.inner(), interval_secs, format, quality, 0)
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
        if path.is_file() && is_screenshot_file(&path) {
            files.push(path.to_string_lossy().to_string());
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
            // Validate date format YYYY-MM-DD（.thumbs 这类目录会被过滤掉）
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

/// 找到磁盘上最新的一张截图：日期目录倒序 + 文件名倒序。
fn latest_screenshot_path() -> Option<PathBuf> {
    let mut dates: Vec<PathBuf> = fs::read_dir(screenshots_dir())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .map(|n| {
                        let s = n.to_string_lossy();
                        s.len() == 10
                            && s.chars().nth(4) == Some('-')
                            && s.chars().nth(7) == Some('-')
                    })
                    .unwrap_or(false)
        })
        .collect();
    dates.sort_by(|a, b| b.cmp(a));

    for dir in dates {
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_screenshot_file(p))
            .collect();
        if files.is_empty() {
            continue;
        }
        files.sort_by(|a, b| b.cmp(a));
        return files.into_iter().next();
    }

    None
}

/// 启动时用：返回最近一张截图，让控制台的「最近截图」不至于空着。
#[tauri::command]
async fn get_latest_screenshot() -> Result<Option<CaptureResult>, String> {
    let Some(path) = latest_screenshot_path() else {
        return Ok(None);
    };
    let filepath = path.to_string_lossy().to_string();
    let thumb_path = thumb_path_for(&path);

    let bytes = if thumb_path.exists() {
        fs::read(&thumb_path).map_err(|e| format!("读取缩略图失败: {}", e))?
    } else {
        tokio::task::spawn_blocking(move || {
            let img = screenshot::load_rgba(&path)?;
            screenshot::make_thumbnail(&img, &thumb_path, THUMB_MAX_WIDTH, THUMB_QUALITY)
        })
        .await
        .map_err(|e| format!("缩略图任务异常: {}", e))??
    };

    Ok(Some(CaptureResult {
        filepath,
        thumbnail: base64::engine::general_purpose::STANDARD.encode(&bytes),
        pending: false,
    }))
}

#[tauri::command]
async fn delete_screenshot(
    filepath: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = ensure_in_screenshots(&filepath)?;
    fs::remove_file(&path).map_err(|e| format!("删除文件失败: {}", e))?;

    // 顺手清掉缩略图与 OCR 记录
    let thumb = thumb_path_for(&path);
    if thumb.exists() {
        fs::remove_file(&thumb).ok();
    }
    state.db.delete_ocr_for_file(&filepath).ok();

    // 清掉去重指纹：否则删掉最后一张后，同一屏再也截不回来
    if let Ok(mut fp) = state.last_fingerprint.lock() {
        *fp = None;
    }

    Ok(())
}

#[tauri::command]
async fn search_screenshots(
    keyword: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    if keyword.trim().is_empty() {
        return Ok(vec![]);
    }
    let rows = state.db.search(keyword.trim())?;
    Ok(rows
        .into_iter()
        .map(|(fp, text, ts)| SearchResult {
            filepath: fp,
            ocr_text: text,
            captured_at: ts,
        })
        .collect())
}

#[tauri::command]
async fn get_ocr_text(
    filepath: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    state.db.get_ocr_for_file(&filepath)
}

/// 手动重新识别一张截图：老图没有识别结果、或想重跑时用。
#[tauri::command]
async fn reocr_screenshot(
    filepath: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    ensure_in_screenshots(&filepath)?;

    // 和自动识别共用同一个信号量，避免手动操作把 CPU 打满
    let _permit = state
        .ocr_slots
        .acquire()
        .await
        .map_err(|e| format!("OCR 队列异常: {}", e))?;

    let fp = filepath.clone();
    let out = tokio::task::spawn_blocking(move || ocr::recognize_text_blocking(&fp))
        .await
        .map_err(|e| format!("OCR 任务异常: {}", e))??;

    let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    state
        .db
        .update_ocr_text(&filepath, &out.flat, &out.display, &ts)?;

    Ok(out.display)
}

/// 开机自启的真实状态以注册表为准，不在 settings.json 里重复存一份。
#[tauri::command]
async fn get_autostart() -> Result<bool, String> {
    Ok(is_autostart_enabled())
}

#[tauri::command]
async fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    apply_autostart(enabled)
}

#[tauri::command]
async fn get_settings() -> Result<Settings, String> {
    Ok(read_settings())
}

#[tauri::command]
async fn save_settings(
    settings: Settings,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化设置失败: {}", e))?;
    fs::write(settings_path(), content).map_err(|e| format!("保存设置失败: {}", e))?;
    // 窗口事件里读的是这份内存值，保存后立即同步
    state
        .close_to_tray
        .store(settings.close_to_tray, Ordering::SeqCst);
    Ok(())
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
        "data_dir": dir.to_string_lossy().to_string(),
    }))
}

/// 统计截图数量与体积。跳过 .thumbs（缩略图），只统计图片文件。
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
            if path.is_dir() {
                if path
                    .file_name()
                    .map(|n| n == THUMBS_DIR_NAME)
                    .unwrap_or(false)
                {
                    continue;
                }
                calc_recursive(&path, size, count);
            } else if path.is_file() && is_screenshot_file(&path) {
                if let Ok(meta) = entry.metadata() {
                    *size += meta.len();
                    *count += 1;
                }
            }
        }
    }
}

fn is_screenshot_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("png") | Some("jpg") | Some("jpeg")
    )
}

// ── Startup ───────────────────────────────────────────────────

/// 启动失败不要静默 panic：写日志 + 标准错误，让用户知道是目录不可写还是数据库坏了。
fn report_fatal(msg: &str) {
    let content = format!(
        "[{}] 启动失败: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg
    );
    eprintln!("{}", content.trim());
    if fs::write(app_dir().join("startup-error.log"), &content).is_err() {
        let _ = fs::write(
            std::env::temp_dir().join("screen-snapshot-startup-error.log"),
            &content,
        );
    }
}

// ── Main ──────────────────────────────────────────────────────

fn main() {
    // 由开机启动项拉起时，窗口只进托盘，不打扰用户
    let start_hidden = std::env::args().any(|a| a == AUTOSTART_ARG);

    let state = match AppState::new() {
        Ok(state) => state,
        Err(e) => {
            report_fatal(&e);
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        // 必须第一个注册：第二个实例启动时会在这里被拦下，
        // 然后把已有窗口唤到前台，避免两份定时任务同时写 screenshots/。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 开机启动项重复触发时不打扰用户（第一个实例可能正安静待在托盘里）
            if argv.iter().any(|a| a == AUTOSTART_ARG) {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(state)
        .setup(move |app| {
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("Screen Snap - 屏幕快照记录工具")
                .menu(&menu)
                .show_menu_on_left_click(false) // 左键不弹出菜单
                .on_menu_event(|_app, event| match event.id().0.as_str() {
                    "show" => {
                        if let Some(window) = _app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        // Close window first, then exit cleanly
                        if let Some(window) = _app.get_webview_window("main") {
                            let _ = window.destroy();
                        }
                        _app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click on release only (fires once, not on both Down + Up)
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            if window.is_visible().unwrap_or(false)
                                && window.is_focused().unwrap_or(false)
                            {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // 窗口默认是隐藏的（tauri.conf.json 里 visible: false），这里决定要不要显示：
            // 开机启动 → 只留托盘图标；手动打开 → 正常显示。
            if let Some(window) = app.get_webview_window("main") {
                if start_hidden {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            // 已经开了自启的，顺便把注册表里的路径刷成当前 exe：
            // 便携版换目录后启动项依然有效。
            if is_autostart_enabled() {
                let _ = apply_autostart(true);
            }

            // 「启动时自动开始记录」：开机启动和手动打开都生效
            let startup = read_settings();
            if startup.auto_start_capture {
                let st = app.state::<AppState>();
                if let Err(e) = start_capture_loop(
                    app.handle(),
                    st.inner(),
                    startup.interval_secs,
                    startup.format.clone(),
                    startup.quality,
                    AUTO_START_DELAY_MS,
                ) {
                    eprintln!("自动开始记录失败: {}", e);
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = app
                    .state::<AppState>()
                    .close_to_tray
                    .load(Ordering::SeqCst);

                if to_tray {
                    // 立刻收进托盘。
                    // 不要用「延时隐藏」：用户可能在延时期间从托盘重新打开窗口，
                    // 延时任务到点会把刚打开的窗口又关一次。
                    api.prevent_close();
                    let _ = window.hide();
                    let _ = app.emit("hidden-to-tray", ());
                }
                // close_to_tray = false 时不拦截，窗口关闭后应用随之退出
            }
        })
        .invoke_handler(tauri::generate_handler![
            take_screenshot,
            read_screenshot_data,
            read_thumbnail_data,
            open_screenshot,
            reveal_screenshot,
            start_capture,
            stop_capture,
            get_capture_status,
            get_screenshots_by_date,
            get_dates_with_screenshots,
            get_latest_screenshot,
            delete_screenshot,
            search_screenshots,
            get_ocr_text,
            reocr_screenshot,
            get_settings,
            save_settings,
            get_autostart,
            set_autostart_enabled,
            get_storage_info,
        ])
        .run(tauri::generate_context!())
        .expect("启动应用失败");
}
