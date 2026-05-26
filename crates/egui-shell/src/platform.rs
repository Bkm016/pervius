//! 平台特定逻辑
//!
//! @author sky

use eframe::egui;

/// 编译期嵌入的应用图标（RGBA）
fn app_icon() -> egui::IconData {
    let png = include_bytes!("../icon.png");
    let img = image::load_from_memory(png)
        .expect("failed to decode icon.png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}

/// 构建平台适配的 ViewportBuilder
pub fn viewport(title: &str, size: [f32; 2]) -> egui::ViewportBuilder {
    let mut vp = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size([640.0, 400.0])
        .with_icon(app_icon());
    // macOS: 保留原生交通灯，内容延伸到标题栏
    #[cfg(target_os = "macos")]
    {
        vp = vp
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(true);
    }
    // Windows / Linux: 完全自绘标题栏
    #[cfg(not(target_os = "macos"))]
    {
        vp = vp.with_decorations(false);
    }
    // 先隐藏窗口，渲染一帧后再显示，避免无框窗口出现黑色闪烁
    vp = vp.with_visible(false);
    vp
}

/// macOS 交通灯按钮微调位置（默认 {7, 13}，增加边距）
#[cfg(target_os = "macos")]
pub fn set_traffic_lights() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    #[repr(C)]
    struct NSPoint {
        x: f64,
        y: f64,
    }

    unsafe impl objc2::Encode for NSPoint {
        const ENCODING: objc2::Encoding =
            objc2::Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }

    unsafe {
        let app: *const AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let window: *const AnyObject = msg_send![app, keyWindow];
        if window.is_null() {
            return;
        }
        let point = NSPoint { x: 12.0, y: 10.0 };
        let _: () = msg_send![window, setTrafficLightPosition: point];
    }
}

/// Windows 11 圆角（Build 22000+），旧版静默忽略
#[cfg(target_os = "windows")]
pub fn enable_rounded_corners(title: &str) {
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            dw_attribute: u32,
            pv_attribute: *const c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
    }

    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: u32 = 2;

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd == 0 {
            return;
        }
        let preference = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}
