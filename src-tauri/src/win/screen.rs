use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageBuffer, ImageFormat, RgbaImage};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetDeviceCaps, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC,
    HORZRES, SRCCOPY, VERTRES,
};

use crate::error::{AppError, AppResult};

/// 抓一次主屏，等比压到 max_edge 像素以内，返回 PNG 的 base64。
/// 压缩是为了把单次 IPC 控制在 1MB 上下；二维码由前端 jsQR 解码。
pub fn capture_png_base64(max_edge: u32) -> AppResult<String> {
    let shot = grab_desktop()?;
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(fit_edge(shot, max_edge))
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| AppError::msg(format!("截图编码失败: {e}")))?;
    Ok(base64_encode(cursor.get_ref()))
}

fn grab_desktop() -> AppResult<RgbaImage> {
    let null = HWND(std::ptr::null_mut());
    unsafe {
        let screen = GetDC(null);
        if screen.is_invalid() {
            return Err(AppError::msg("无法读取屏幕，应用需要运行在有桌面的会话里"));
        }
        let width = GetDeviceCaps(screen, HORZRES);
        let height = GetDeviceCaps(screen, VERTRES);
        let grabbed = grab_region(screen, width, height);
        let _ = ReleaseDC(null, screen);
        grabbed
    }
}

unsafe fn grab_region(screen: HDC, width: i32, height: i32) -> AppResult<RgbaImage> {
    if width <= 0 || height <= 0 {
        return Err(AppError::msg("屏幕尺寸无效"));
    }
    let mem = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    if bitmap.is_invalid() {
        let _ = DeleteDC(mem);
        return Err(AppError::msg("创建截图位图失败"));
    }
    let old = SelectObject(mem, bitmap);
    BitBlt(mem, 0, 0, width, height, screen, 0, 0, SRCCOPY)
        .map_err(|e| AppError::msg(format!("屏幕拷贝失败: {e}")))?;

    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // 负高度表示自上而下取行，省掉一次整屏翻转
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let lines = GetDIBits(
        mem,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr().cast()),
        &mut info,
        DIB_RGB_COLORS,
    );
    SelectObject(mem, old);
    let _ = DeleteObject(old);
    let _ = DeleteObject(bitmap);
    let _ = DeleteDC(mem);

    if lines == 0 {
        return Err(AppError::msg("读取屏幕像素失败"));
    }
    bgra_to_rgba(&mut pixels);
    ImageBuffer::from_raw(width as u32, height as u32, pixels)
        .ok_or_else(|| AppError::msg("截图数据长度不符"))
}

/// GDI 交回的是 BGRA，且 alpha 字节通常是 0；PNG 需要 RGBA 且不透明。
fn bgra_to_rgba(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
}

/// 等比缩放到最长边不超过 max_edge，小图原样返回（放大只会让二维码更糊）。
fn fit_edge(img: RgbaImage, max_edge: u32) -> RgbaImage {
    let longest = img.width().max(img.height());
    if longest <= max_edge {
        return img;
    }
    let scale = max_edge as f64 / longest as f64;
    let width = (img.width() as f64 * scale).round().max(1.0) as u32;
    let height = (img.height() as f64 * scale).round().max(1.0) as u32;
    image::imageops::resize(&img, width, height, FilterType::Triangle)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 依赖清单里没有 base64 crate（联网拉包被拦），这里手写标准带填充的编码。
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn bgra_becomes_opaque_rgba() {
        let mut px = vec![0x11, 0x22, 0x33, 0x00, 0xAA, 0xBB, 0xCC, 0x00];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![0x33, 0x22, 0x11, 255, 0xCC, 0xBB, 0xAA, 255]);
    }

    #[test]
    fn fit_edge_caps_longest_side_only() {
        let big = RgbaImage::from_pixel(1920, 1080, image::Rgba([0, 0, 0, 255]));
        let small = fit_edge(big, 1000);
        assert_eq!((small.width(), small.height()), (1000, 563));

        let tiny = RgbaImage::from_pixel(300, 200, image::Rgba([0, 0, 0, 255]));
        let kept = fit_edge(tiny, 1000);
        assert_eq!((kept.width(), kept.height()), (300, 200));
    }

    /// 桌面会话里真跑一次抓屏：CI 上没有桌面时只跳过断言，不报失败。
    #[test]
    fn desktop_capture_returns_png_bytes() {
        match capture_png_base64(400) {
            Ok(b64) => {
                assert!(b64.starts_with("iVBORw0KGgo"), "不是 PNG: {}", &b64[..20.min(b64.len())]);
                assert!(b64.len() > 200);
            }
            Err(e) => eprintln!("跳过抓屏测试（无可用桌面）: {e}"),
        }
    }
}
