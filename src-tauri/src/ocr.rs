use image::GenericImageView;
use std::path::Path;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

/// Run Windows.Media.Ocr on a saved image file.
/// This function is blocking — call it inside `tokio::task::spawn_blocking`.
pub fn recognize_text_blocking(filepath: &str) -> Result<String, String> {
    let fp = Path::new(filepath);
    if !fp.exists() {
        return Err("文件不存在".to_string());
    }

    // 1. Load image and encode as BMP in memory
    let img = image::open(fp).map_err(|e| format!("打开图片失败: {}", e))?;
    let (width, height) = img.dimensions();
    let rgba = img.to_rgba8();

    let mut bmp_bytes = Vec::new();
    let mut encoder = image::codecs::bmp::BmpEncoder::new(&mut bmp_bytes);
    encoder
        .encode(
            rgba.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("BMP编码失败: {}", e))?;

    // 2. Write BMP data to InMemoryRandomAccessStream
    let stream = InMemoryRandomAccessStream::new()
        .map_err(|e| format!("创建内存流失败: {}", e))?;

    let writer = DataWriter::CreateDataWriter(&stream)
        .map_err(|e| format!("创建 DataWriter 失败: {}", e))?;

    writer
        .WriteBytes(&bmp_bytes)
        .map_err(|e| format!("写入字节失败: {}", e))?;

    writer
        .StoreAsync()
        .map_err(|e| format!("StoreAsync 失败: {}", e))?
        .get()
        .map_err(|e| format!("等待 StoreAsync 失败: {}", e))?;

    writer
        .DetachStream()
        .map_err(|e| format!("DetachStream 失败: {}", e))?;

    // Seek stream to beginning
    stream
        .Seek(0)
        .map_err(|e| format!("Seek 失败: {}", e))?;

    // 3. Decode BMP from stream into SoftwareBitmap
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|e| format!("创建解码器失败: {}", e))?
        .get()
        .map_err(|e| format!("等待解码器失败: {}", e))?;

    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| format!("获取位图失败: {}", e))?
        .get()
        .map_err(|e| format!("等待位图失败: {}", e))?;

    // 4. Create OcrEngine
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("创建OCR引擎失败: {}", e))?;

    // 5. Run OCR
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| format!("OCR识别调用失败: {}", e))?
        .get()
        .map_err(|e| format!("OCR识别执行失败: {}", e))?;

    // 6. Extract and clean text: remove all whitespace characters
    let raw = result
        .Text()
        .map_err(|e| format!("获取OCR文本失败: {}", e))?
        .to_string();
    let text = clean_ocr_text(&raw);

    // 7. Clean up
    bitmap
        .Close()
        .map_err(|e| format!("释放位图失败: {}", e))?;

    Ok(text)
}

/// Remove all whitespace and control characters from OCR output.
/// This improves search accuracy by eliminating garbage spacing that OCR produces.
fn clean_ocr_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}
