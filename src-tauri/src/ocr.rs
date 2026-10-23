use std::sync::OnceLock;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// OCR 结果。
/// - `display`：保留换行，给灯箱阅读/复制用
/// - `flat`：去掉所有空白，给搜索用（避免换行把关键词切断）
pub struct OcrOutput {
    pub display: String,
    pub flat: String,
}

/// 引擎创建有固定开销（百毫秒级），全局复用一份。
static ENGINE: OnceLock<OcrEngine> = OnceLock::new();

fn engine() -> Result<&'static OcrEngine, String> {
    if let Some(e) = ENGINE.get() {
        return Ok(e);
    }
    let created = create_engine()?;
    Ok(ENGINE.get_or_init(|| created))
}

/// 用系统用户语言创建引擎。
/// 注：`TryCreateFromLanguage` / `AvailableRecognizerLanguages` 需要 windows crate 的
/// `Globalization` feature（会让整个 crate 重编译），这里不引入——中文系统自带
/// zh-Hans 的 OCR 语言包，用户语言路径足够用。
fn create_engine() -> Result<OcrEngine, String> {
    OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
        format!(
            "创建 OCR 引擎失败: {}。请确认系统已安装识别语言包\
             （设置 → 时间和语言 → 语言 → 中文 → 可选功能 → 光学字符识别）。",
            e
        )
    })
}

/// 识别一张已落盘的截图。阻塞调用，请放在 `spawn_blocking` 里。
///
/// 直接把原图字节（PNG/JPG）交给 BitmapDecoder 解码：
/// 不再用 image crate 解码再编码成 BMP，省掉一次整图解码 + 一次编码 + 一块 15MB 缓冲。
pub fn recognize_text_blocking(filepath: &str) -> Result<OcrOutput, String> {
    let bytes = std::fs::read(filepath).map_err(|e| format!("读取图片失败: {}", e))?;

    let stream = InMemoryRandomAccessStream::new().map_err(|e| format!("创建内存流失败: {}", e))?;
    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|e| format!("创建 DataWriter 失败: {}", e))?;

    writer
        .WriteBytes(&bytes)
        .map_err(|e| format!("写入字节失败: {}", e))?;
    writer
        .StoreAsync()
        .map_err(|e| format!("StoreAsync 失败: {}", e))?
        .get()
        .map_err(|e| format!("等待 StoreAsync 失败: {}", e))?;
    writer
        .DetachStream()
        .map_err(|e| format!("DetachStream 失败: {}", e))?;
    stream
        .Seek(0)
        .map_err(|e| format!("Seek 失败: {}", e))?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("创建解码器失败: {}", e))?
        .get()
        .map_err(|e| format!("等待解码器失败: {}", e))?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| format!("获取位图失败: {}", e))?
        .get()
        .map_err(|e| format!("等待位图失败: {}", e))?;

    let result = engine()?
        .RecognizeAsync(&bitmap)
        .map_err(|e| format!("OCR 调用失败: {}", e))?
        .get()
        .map_err(|e| format!("OCR 执行失败: {}", e))?;

    let raw = result
        .Text()
        .map_err(|e| format!("获取识别文本失败: {}", e))?
        .to_string();

    let _ = bitmap.Close();

    Ok(OcrOutput {
        display: tidy_text(&raw),
        flat: flatten_text(&raw),
    })
}

/// 逐行 trim、丢掉空行、去掉汉字之间的多余空格，保留换行结构。
///
/// Windows OCR 会把中文逐字分开输出（`腾 讠 丑 会 议`），直接展示很难读；
/// 把「汉字 + 空格 + 汉字」里的空格去掉后就是 `腾讠丑会议`（误识别的字改不了，
/// 但至少不会被空格切碎）。英文单词之间的空格保持不动。
fn tidy_text(text: &str) -> String {
    text.lines()
        .map(|line| collapse_han_spaces(line.trim()))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn collapse_han_spaces(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut prev: Option<char> = None;

    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            if let (Some(p), Some(n)) = (prev, chars.get(i + 1).copied()) {
                if is_han(p) && is_han(n) {
                    continue;
                }
            }
        }
        out.push(c);
        prev = Some(c);
    }

    out
}

/// 去掉所有空白与控制字符，用于搜索匹配。
fn flatten_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}
