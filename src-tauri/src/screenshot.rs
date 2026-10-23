use image::RgbaImage;
use screenshots::Screen;
use std::path::Path;

/// 抓取主显示器，返回 RGBA 原始字节。
/// 注意：screenshots 用的是 image 0.24，本项目用 0.25，两个类型不通用，
/// 所以这里只取 `into_raw()` 的字节（零拷贝，直接移交所有权）。
pub fn capture_screen() -> Result<(Vec<u8>, u32, u32), String> {
    let screens = Screen::all().map_err(|e| format!("枚举显示器失败: {}", e))?;
    let screen = screens.first().ok_or("未检测到显示器")?;
    let image = screen.capture().map_err(|e| format!("截屏失败: {}", e))?;

    let width = image.width();
    let height = image.height();
    // screenshots 内部已把 BGRA 转成 RGBA，直接移交缓冲区，不做逐像素拷贝
    Ok((image.into_raw(), width, height))
}

/// 把整图编码成 PNG / JPEG 字节。调用方负责写盘，不需要回读文件。
pub fn encode_image(img: &RgbaImage, format: &str, quality: u8) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => {
            let rgb = to_rgb_bytes(img.as_raw());
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
            encoder
                .encode(&rgb, img.width(), img.height(), image::ExtendedColorType::Rgb8)
                .map_err(|e| format!("JPEG 编码失败: {}", e))?;
        }
        "png" => {
            img.write_to(
                &mut std::io::Cursor::new(&mut output),
                image::ImageFormat::Png,
            )
            .map_err(|e| format!("PNG 编码失败: {}", e))?;
        }
        _ => return Err(format!("不支持的图片格式: {}", format)),
    }

    Ok(output)
}

/// 解码图片文件为 RgbaImage。
pub fn load_rgba(path: &Path) -> Result<RgbaImage, String> {
    let img = image::open(path).map_err(|e| format!("打开图片失败: {}", e))?;
    Ok(img.to_rgba8())
}

/// 缩放 + JPEG 编码，不碰磁盘。
/// 截屏后立刻用它拿到预览图，原图编码/落盘交给后台线程。
pub fn encode_thumbnail(
    img: &RgbaImage,
    max_width: u32,
    quality: u8,
) -> Result<Vec<u8>, String> {
    let (width, height) = img.dimensions();

    let (tw, th) = if width > max_width {
        let th = ((height as f64) * (max_width as f64) / (width as f64))
            .round()
            .max(1.0) as u32;
        (max_width, th)
    } else {
        (width, height)
    };

    let resized = if tw == width && th == height {
        img.clone()
    } else {
        image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle)
    };

    let rgb = to_rgb_bytes(resized.as_raw());
    let mut output = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
    encoder
        .encode(&rgb, tw, th, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("缩略图编码失败: {}", e))?;

    Ok(output)
}

/// 生成缩略图并写盘，返回字节。
pub fn make_thumbnail(
    img: &RgbaImage,
    path: &Path,
    max_width: u32,
    quality: u8,
) -> Result<Vec<u8>, String> {
    let output = encode_thumbnail(img, max_width, quality)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建缩略图目录失败: {}", e))?;
    }
    std::fs::write(path, &output).map_err(|e| format!("写入缩略图失败: {}", e))?;

    Ok(output)
}

fn to_rgb_bytes(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect()
}

// ── 重复画面判定 ──────────────────────────────────────────────

/// 指纹尺寸：64×36 灰度，共 2304 个采样点。
/// 缩到这么小之后，任务栏时钟、光标闪烁这类细微变化会被平均掉，
/// 而页面结构变化会明显体现出来。
pub const FINGERPRINT_W: u32 = 64;
pub const FINGERPRINT_H: u32 = 36;

/// 把整屏压成一张小灰度图，用于和上一张比较。
pub fn fingerprint(img: &RgbaImage) -> Vec<u8> {
    let small = image::imageops::resize(
        img,
        FINGERPRINT_W,
        FINGERPRINT_H,
        image::imageops::FilterType::Triangle,
    );

    small
        .pixels()
        .map(|p| {
            // 人眼对绿色最敏感，用标准亮度权重
            ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8
        })
        .collect()
}

/// 两张指纹的平均像素差（0 = 完全一样，255 = 完全不同）。
pub fn fingerprint_diff(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::MAX;
    }
    let sum: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum();
    sum as f64 / a.len() as f64
}
