use image::{ImageBuffer, RgbaImage};
use screenshots::Screen;
use std::path::PathBuf;

/// Captures the primary monitor and returns (RGBA pixel data, width, height).
pub fn capture_screen() -> Result<(Vec<u8>, u32, u32), String> {
    let screens = Screen::all().map_err(|e| format!("枚举显示器失败: {}", e))?;
    let screen = screens.first().ok_or("未检测到显示器")?;
    let image = screen
        .capture()
        .map_err(|e| format!("截屏失败: {}", e))?;

    let width = image.width();
    let height = image.height();

    // The screenshots crate already converts BGRA→RGBA internally.
    // pixel.0 is already correct RGBA: [R, G, B, A].
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for pixel in image.pixels() {
        rgba.extend_from_slice(&pixel.0);
    }

    Ok((rgba, width, height))
}

/// Saves RGBA pixel data to an image file with the specified format and quality.
pub fn save_screenshot(
    data: &[u8],
    width: u32,
    height: u32,
    path: &PathBuf,
    format: &str,
    quality: u8,
) -> Result<(), String> {
    let img: RgbaImage =
        ImageBuffer::from_raw(width, height, data.to_vec()).ok_or("无法创建图像缓冲区")?;

    match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => {
            // Convert RGBA to RGB for JPEG encoding
            let rgb: Vec<u8> = data
                .chunks(4)
                .flat_map(|c| [c[0], c[1], c[2]])
                .collect();

            let mut output = Vec::new();
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, quality);
            encoder
                .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
                .map_err(|e| format!("JPEG 编码失败: {}", e))?;

            std::fs::write(path, &output)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }
        "png" => {
            img.save(path)
                .map_err(|e| format!("PNG 保存失败: {}", e))?;
        }
        _ => return Err(format!("不支持的图片格式: {}", format)),
    }

    Ok(())
}
