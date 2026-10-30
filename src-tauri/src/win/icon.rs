//! 把一个文件/快捷方式的图标抽成 PNG 存进缓存目录：shell 负责按 IconLocation
//! （含「dll 里第 N 个图标」那种写法）算出该抽谁，我们只管落盘。
use std::ffi::c_void;

use image::{ImageBuffer, Rgba, RgbaImage};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, RGBQUAD,
};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
    SIIGBF_RESIZETOFIT,
};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::SIZE;

/// 从可执行文件或快捷方式提取 64x64 图标并保存为 PNG。
pub fn extract_icon_to_png(target: &str, out_png: &str) -> Result<(), String> {
    extract_icon_at(target, out_png, 64)
}

/// 同一件事，尺寸自己定：文件页列表里只要 32，抽 64 再缩放是白干活（一张列表几百行）
pub fn extract_icon_at(target: &str, out_png: &str, size: u32) -> Result<(), String> {
    let side = size.max(16) as i32;
    unsafe {
        let wpath: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wpath.as_ptr()), None)
            .map_err(|e| format!("创建 ShellItem 失败: {e}"))?;
        let factory: IShellItemImageFactory = item
            .cast()
            .map_err(|e| format!("获取图像工厂失败: {e}"))?;

        let hbmp: HBITMAP = factory
            .GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT)
            .map_err(|e| format!("提取图标失败: {e}"))?;

        let r = bitmap_to_png(hbmp, out_png);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        r
    }
}

unsafe fn bitmap_to_png(hbmp: HBITMAP, out: &str) -> Result<(), String> {
    let mut bmp = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(hbmp.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bmp as *mut _ as *mut c_void),
    );
    if got == 0 {
        return Err("读取位图信息失败".into());
    }
    let width = bmp.bmWidth.max(1) as u32;
    let height = bmp.bmHeight.max(1) as u32;

    let hdc: HDC = CreateCompatibleDC(None);

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default(); 1],
    };

    let mut buf = vec![0u8; (width * height * 4) as usize];
    let lines = GetDIBits(
        hdc,
        hbmp,
        0,
        height,
        Some(buf.as_mut_ptr() as *mut c_void),
        &mut bmi as *mut BITMAPINFO,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(hdc);

    if lines == 0 {
        return Err("读取像素数据失败".into());
    }

    let mut img: RgbaImage = ImageBuffer::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let idx = ((y * width + x) * 4) as usize;
        let b = buf[idx];
        let g = buf[idx + 1];
        let r = buf[idx + 2];
        let a = buf[idx + 3];
        *pixel = Rgba([r, g, b, a]);
    }
    img.save(out).map_err(|e| format!("保存 PNG 失败: {e}"))
}