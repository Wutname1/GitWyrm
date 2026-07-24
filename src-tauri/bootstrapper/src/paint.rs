use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;

// GitWyrm theme: near-black bg, off-white text, teal accent
pub const COLOR_BG: COLORREF = COLORREF(rgb(10, 14, 14));
pub const COLOR_PANEL: COLORREF = COLORREF(rgb(15, 20, 20));
pub const COLOR_TEXT: COLORREF = COLORREF(rgb(246, 246, 246));
pub const COLOR_SUBTEXT: COLORREF = COLORREF(rgb(160, 170, 168));
pub const COLOR_ACCENT: COLORREF = COLORREF(rgb(45, 212, 191)); // teal #2dd4bf
pub const COLOR_BAR_BG: COLORREF = COLORREF(rgb(30, 38, 37));
pub const COLOR_ERROR: COLORREF = COLORREF(rgb(220, 90, 90));
pub const COLOR_HOVER: COLORREF = COLORREF(rgb(45, 212, 191));
pub const COLOR_DIVIDER: COLORREF = COLORREF(rgb(40, 48, 47));
pub const COLOR_SCRIM: COLORREF = COLORREF(rgb(4, 6, 6));
pub const COLOR_BAR_START: [u8; 3] = [16, 185, 160]; // teal
pub const COLOR_BAR_END: [u8; 3] = [45, 212, 191]; // lighter teal

const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

fn lerp_color(a: [u8; 3], b: [u8; 3], t: f64) -> COLORREF {
    let r = (a[0] as f64 * (1.0 - t) + b[0] as f64 * t) as u8;
    let g = (a[1] as f64 * (1.0 - t) + b[1] as f64 * t) as u8;
    let bl = (a[2] as f64 * (1.0 - t) + b[2] as f64 * t) as u8;
    COLORREF(rgb(r, g, bl))
}

pub fn fill_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        FillRect(hdc, &rc, brush);
        let _ = DeleteObject(brush.into());
    }
}

pub fn fill_rounded_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, radius: i32, color: COLORREF) {
    unsafe {
        let rgn = CreateRoundRectRgn(x, y, x + w, y + h, radius * 2, radius * 2);
        let brush = CreateSolidBrush(color);
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(rgn.into());
        let _ = DeleteObject(brush.into());
    }
}

pub fn fill_gradient_bar(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color_start: [u8; 3],
    color_end: [u8; 3],
) {
    unsafe {
        let mem_dc = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem_dc, bmp.into());

        for i in 0..w {
            let t = i as f64 / w.max(1) as f64;
            let c = lerp_color(color_start, color_end, t);
            let brush = CreateSolidBrush(c);
            let rc = RECT {
                left: i,
                top: 0,
                right: i + 1,
                bottom: h,
            };
            FillRect(mem_dc, &rc, brush);
            let _ = DeleteObject(brush.into());
        }

        let rgn = CreateRoundRectRgn(x, y, x + w, y + h, radius * 2, radius * 2);
        SelectClipRgn(hdc, Some(rgn));
        let _ = BitBlt(hdc, x, y, w, h, Some(mem_dc), 0, 0, SRCCOPY);
        SelectClipRgn(hdc, None);

        let _ = DeleteObject(rgn.into());
        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem_dc);
    }
}

/// Draw text and return the width it occupied (for chaining colored segments)
pub fn draw_text_w(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT, color: COLORREF) -> i32 {
    if text.is_empty() {
        return 0;
    }
    unsafe {
        let old_font = SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let len = wide.len();
        let mut rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        DrawTextW(hdc, &mut wide[..len], &mut rc, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT);
        let text_w = rc.right - rc.left;

        rc.right = x + w;
        rc.bottom = y + h;
        DrawTextW(hdc, &mut wide[..len], &mut rc, DT_LEFT | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old_font);
        text_w
    }
}

pub fn draw_text(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT, color: COLORREF) {
    draw_text_w(hdc, text, x, y, w, h, font, color);
}

pub fn draw_text_center(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT, color: COLORREF) {
    if text.is_empty() {
        return;
    }
    unsafe {
        let old_font = SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let len = wide.len();
        let mut rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        DrawTextW(hdc, &mut wide[..len], &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old_font);
    }
}

pub fn draw_text_wrap(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT, color: COLORREF) {
    if text.is_empty() {
        return;
    }
    unsafe {
        let old_font = SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let len = wide.len();
        let mut rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        DrawTextW(hdc, &mut wide[..len], &mut rc, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(hdc, old_font);
    }
}

pub fn create_font(size: i32, weight: u32) -> HFONT {
    create_font_named(size, weight, "Segoe UI")
}

pub fn create_font_named(size: i32, weight: u32, face: &str) -> HFONT {
    unsafe {
        let mut wide: Vec<u16> = face.encode_utf16().chain(std::iter::once(0)).collect();
        wide.resize(32, 0);
        CreateFontW(
            size,
            0,
            0,
            0,
            weight as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(wide.as_ptr()),
        )
    }
}


struct RgbBitmap {
    width: i32,
    height: i32,
    // BGR pixel data, bottom-up row order (DIB format)
    pixels: Vec<u8>,
}

fn decode_png_to_dib(bytes: &[u8]) -> RgbBitmap {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().expect("bad png");
    let info = reader.info();
    let width = info.width as i32;
    let height = info.height as i32;
    let color_type = info.color_type;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("bad png frame");
    let data = &buf[..frame.buffer_size()];

    let row_bytes = ((width * 3 + 3) & !3) as usize;
    let mut pixels = vec![0u8; row_bytes * height as usize];

    for y in 0..height as usize {
        let dst_y = (height as usize - 1) - y; // flip vertically
        for x in 0..width as usize {
            let (r, g, b) = match color_type {
                png::ColorType::Rgb => {
                    let i = y * width as usize * 3 + x * 3;
                    (data[i], data[i + 1], data[i + 2])
                }
                png::ColorType::Rgba => {
                    let i = y * width as usize * 4 + x * 4;
                    (data[i], data[i + 1], data[i + 2])
                }
                png::ColorType::Grayscale => {
                    let i = y * width as usize;
                    (data[i], data[i], data[i])
                }
                _ => (0, 0, 0),
            };
            let dst = dst_y * row_bytes + x * 3;
            pixels[dst] = b;
            pixels[dst + 1] = g;
            pixels[dst + 2] = r;
        }
    }

    RgbBitmap { width, height, pixels }
}

fn draw_dib_stretched(hdc: HDC, bmp: &RgbBitmap, x: i32, y: i32, dst_w: i32, dst_h: i32) {
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bmp.width,
                biHeight: bmp.height,
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        SetStretchBltMode(hdc, HALFTONE);
        StretchDIBits(
            hdc,
            x,
            y,
            dst_w,
            dst_h,
            0,
            0,
            bmp.width,
            bmp.height,
            Some(bmp.pixels.as_ptr() as *const _),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

static SPLASH_PNG: &[u8] = include_bytes!("images/splash.png");
static SPLASH_DECODED: OnceLock<RgbBitmap> = OnceLock::new();

fn decode_splash() -> &'static RgbBitmap {
    SPLASH_DECODED.get_or_init(|| decode_png_to_dib(SPLASH_PNG))
}

/// Draw the left-panel splash art, cropped to fill the panel (cover-fit).
pub fn draw_splash(hdc: HDC, x: i32, y: i32, w: i32, h: i32) {
    let splash = decode_splash();

    let scale = (w as f64 / splash.width as f64).max(h as f64 / splash.height as f64);
    let draw_w = (splash.width as f64 * scale) as i32;
    let draw_h = (splash.height as f64 * scale) as i32;
    let offset_x = x - (draw_w - w) / 2;
    let offset_y = y - (draw_h - h) / 2;

    unsafe {
        let rgn = CreateRectRgn(x, y, x + w, y + h);
        SelectClipRgn(hdc, Some(rgn));
        draw_dib_stretched(hdc, splash, offset_x, offset_y, draw_w, draw_h);
        SelectClipRgn(hdc, None);
        let _ = DeleteObject(rgn.into());
    }
}

struct BgraBitmap {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

/// Decode a PNG into a bottom-up, premultiplied-BGRA bitmap ready for AlphaBlend.
fn decode_png_to_bgra(bytes: &[u8]) -> BgraBitmap {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().expect("bad png");
    let info = reader.info();
    let width = info.width as i32;
    let height = info.height as i32;
    let color_type = info.color_type;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("bad png frame");
    let data = &buf[..frame.buffer_size()];

    let row_bytes = (width * 4) as usize;
    let mut pixels = vec![0u8; row_bytes * height as usize];

    for y in 0..height as usize {
        let dst_y = (height as usize - 1) - y;
        for x in 0..width as usize {
            let (r, g, b, a) = match color_type {
                png::ColorType::Rgb => {
                    let i = y * width as usize * 3 + x * 3;
                    (data[i], data[i + 1], data[i + 2], 255u8)
                }
                png::ColorType::Rgba => {
                    let i = y * width as usize * 4 + x * 4;
                    (data[i], data[i + 1], data[i + 2], data[i + 3])
                }
                _ => (0, 0, 0, 255),
            };
            let af = a as f64 / 255.0;
            let dst = dst_y * row_bytes + x * 4;
            pixels[dst] = (b as f64 * af) as u8;
            pixels[dst + 1] = (g as f64 * af) as u8;
            pixels[dst + 2] = (r as f64 * af) as u8;
            pixels[dst + 3] = a;
        }
    }

    BgraBitmap { width, height, pixels }
}

/// Alpha-blend a premultiplied-BGRA bitmap onto `hdc` at (x, y), scaled to draw_w x draw_h.
fn blit_bgra(hdc: HDC, bmp: &BgraBitmap, x: i32, y: i32, draw_w: i32, draw_h: i32) {
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bmp.width,
                biHeight: bmp.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mem_dc = CreateCompatibleDC(Some(hdc));
        let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0)
            .expect("CreateDIBSection failed");
        let old = SelectObject(mem_dc, dib.into());

        std::ptr::copy_nonoverlapping(bmp.pixels.as_ptr(), bits_ptr as *mut u8, bmp.pixels.len());

        let bf = BLENDFUNCTION {
            BlendOp: 0,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1,
        };

        SetStretchBltMode(hdc, HALFTONE);
        let _ = AlphaBlend(hdc, x, y, draw_w, draw_h, mem_dc, 0, 0, bmp.width, bmp.height, bf);

        SelectObject(mem_dc, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
    }
}

static LOGO_PNG: &[u8] = include_bytes!("images/logo.png");
static LOGO_DECODED: OnceLock<BgraBitmap> = OnceLock::new();

fn decode_logo() -> &'static BgraBitmap {
    LOGO_DECODED.get_or_init(|| decode_png_to_bgra(LOGO_PNG))
}

pub fn draw_logo(hdc: HDC, x: i32, y: i32, max_w: i32, max_h: i32) {
    let logo = decode_logo();
    let scale = (max_w as f64 / logo.width as f64).min(max_h as f64 / logo.height as f64);
    let draw_w = (logo.width as f64 * scale) as i32;
    let draw_h = (logo.height as f64 * scale) as i32;
    blit_bgra(hdc, logo, x, y, draw_w, draw_h);
}

static WORDMARK_PNG: &[u8] = include_bytes!("images/wordmark.png");
static WORDMARK_24_PNG: &[u8] = include_bytes!("images/wordmark-24.png");
static WORDMARK_40_PNG: &[u8] = include_bytes!("images/wordmark-40.png");
static WORDMARK_DECODED: OnceLock<BgraBitmap> = OnceLock::new();
static WORDMARK_24_DECODED: OnceLock<BgraBitmap> = OnceLock::new();
static WORDMARK_40_DECODED: OnceLock<BgraBitmap> = OnceLock::new();

fn decode_wordmark() -> &'static BgraBitmap {
    WORDMARK_DECODED.get_or_init(|| decode_png_to_bgra(WORDMARK_PNG))
}

fn decode_wordmark_24() -> &'static BgraBitmap {
    WORDMARK_24_DECODED.get_or_init(|| decode_png_to_bgra(WORDMARK_24_PNG))
}

fn decode_wordmark_40() -> &'static BgraBitmap {
    WORDMARK_40_DECODED.get_or_init(|| decode_png_to_bgra(WORDMARK_40_PNG))
}

/// Draw the "GitWyrm" wordmark image scaled to `height` pixels tall, top-aligned at (x, y).
/// Returns the width it occupied, so following text can be positioned after it.
pub fn draw_wordmark_img(hdc: HDC, x: i32, y: i32, height: i32) -> i32 {
    // AlphaBlend uses low-quality sampling when it also resizes. Use assets
    // prefiltered to the two UI sizes so their antialiasing reaches the screen
    // unchanged. Keep the full-size fallback for any future call site.
    let wm = match height {
        24 => decode_wordmark_24(),
        40 => decode_wordmark_40(),
        _ => decode_wordmark(),
    };
    let scale = height as f64 / wm.height as f64;
    let draw_w = (wm.width as f64 * scale) as i32;
    blit_bgra(hdc, wm, x, y, draw_w, height);
    draw_w
}

/// Draw an indeterminate progress bar - a glowing segment that slides back and forth
pub fn fill_indeterminate_bar(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color_start: [u8; 3],
    color_end: [u8; 3],
    tick: u64,
) {
    let glow_w = w / 3;
    let cycle = (w - glow_w) * 2;
    if cycle <= 0 {
        return;
    }
    let pos = (tick as i32 * 12) % cycle;
    let glow_x = if pos < cycle / 2 { pos } else { cycle - pos };

    unsafe {
        let rgn = CreateRoundRectRgn(x, y, x + w, y + h, radius * 2, radius * 2);
        SelectClipRgn(hdc, Some(rgn));

        let mem_dc = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, glow_w, h);
        let old = SelectObject(mem_dc, bmp.into());

        for i in 0..glow_w {
            let center = glow_w as f64 / 2.0;
            let dist = ((i as f64 - center) / center).abs();
            let alpha = 1.0 - dist * dist;
            let t = i as f64 / glow_w.max(1) as f64;
            let base = lerp_color(color_start, color_end, t);
            let bg_r = 30u8;
            let bg_g = 38u8;
            let bg_b = 37u8; // COLOR_BAR_BG
            let br = (base.0 & 0xFF) as u8;
            let bg_ = ((base.0 >> 8) & 0xFF) as u8;
            let bb = ((base.0 >> 16) & 0xFF) as u8;
            let r = (bg_r as f64 * (1.0 - alpha) + br as f64 * alpha) as u8;
            let g = (bg_g as f64 * (1.0 - alpha) + bg_ as f64 * alpha) as u8;
            let b = (bg_b as f64 * (1.0 - alpha) + bb as f64 * alpha) as u8;
            let c = COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16);
            let brush = CreateSolidBrush(c);
            let rc = RECT { left: i, top: 0, right: i + 1, bottom: h };
            FillRect(mem_dc, &rc, brush);
            let _ = DeleteObject(brush.into());
        }

        let _ = BitBlt(hdc, x + glow_x, y, glow_w, h, Some(mem_dc), 0, 0, SRCCOPY);

        SelectClipRgn(hdc, None);
        let _ = DeleteObject(rgn.into());
        SelectObject(mem_dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem_dc);
    }
}
