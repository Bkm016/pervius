//! 自定义标题栏：拖拽 + 控制按钮
//!
//! 布局：[菜单栏(外部注入)] ... [标题居中] ... [最小化][最大化][关闭]
//! 拖拽在底层先注册，菜单和按钮在顶层后注册覆盖拖拽。
//!
//! @author sky

use super::app::ShellTheme;
#[cfg(not(target_os = "macos"))]
use super::codicon;
use eframe::egui;

/// caption button 宽度
#[cfg(not(target_os = "macos"))]
const CAPTION_BTN_W: f32 = 46.0;
/// 标题栏 logo 逻辑高度（逻辑像素）
const LOGO_H: f32 = 18.0;

/// 按 LOGO_H * pixels_per_point 光栅化 SVG，缓存在 egui Context 中
fn logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let ppp = ctx.pixels_per_point();
    // 用整数化的 ppp 做缓存 key，避免浮点微抖导致反复重建
    let ppp_key = (ppp * 100.0) as u32;
    let cache_id = egui::Id::new("shell_logo").with(ppp_key);
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(cache_id)) {
        return tex;
    }
    let svg_data = include_bytes!("../logo.svg");
    let tree = resvg::usvg::Tree::from_data(svg_data, &resvg::usvg::Options::default())
        .expect("failed to parse logo.svg");
    let src = tree.size();
    let physical_h = LOGO_H * ppp;
    let scale = physical_h / src.height();
    let w = (src.width() * scale).ceil() as u32;
    let h = physical_h.ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pixmap.data());
    let tex = ctx.load_texture("shell_logo", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(cache_id, tex.clone()));
    tex
}

/// 渲染标题栏，菜单内容由调用方通过闭包注入
pub fn render(
    ui: &mut egui::Ui,
    title: &str,
    theme: &ShellTheme,
    menu_bar: impl FnOnce(&mut egui::Ui),
) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("title_bar")
        .exact_size(theme.title_bar_height)
        .frame(egui::Frame::NONE.fill(theme.bg))
        .show_separator_line(false)
        .show_inside(ui, |ui| {
            let bar = ui.max_rect();
            // 底层：拖拽区域（先注册 = 优先级最低）
            let drag = ui.interact(bar, ui.id().with("drag"), egui::Sense::click_and_drag());
            #[cfg(not(target_os = "macos"))]
            if drag.double_clicked() {
                let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            #[cfg(target_os = "macos")]
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            // 菜单栏（左侧）
            #[cfg(not(target_os = "macos"))]
            let menu_w = bar.width() - CAPTION_BTN_W * 3.0;
            #[cfg(target_os = "macos")]
            let menu_w = bar.width();
            let menu_rect =
                egui::Rect::from_min_size(bar.left_top(), egui::vec2(menu_w, bar.height()));
            ui.new_child(egui::UiBuilder::new().max_rect(menu_rect))
                .horizontal_centered(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    visuals.widgets.inactive.fg_stroke =
                        egui::Stroke::new(1.0, theme.text_secondary);
                    visuals.widgets.hovered.weak_bg_fill = theme.bg_hover;
                    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, theme.text_primary);
                    visuals.widgets.active.weak_bg_fill = theme.bg_hover;
                    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
                    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, theme.text_primary);
                    ui.spacing_mut().item_spacing.x = 0.0;
                    #[cfg(target_os = "macos")]
                    ui.add_space(72.0);
                    #[cfg(not(target_os = "macos"))]
                    ui.add_space(8.0);
                    menu_bar(ui);
                });
            // caption buttons（右侧，仅 Windows/Linux）
            #[cfg(not(target_os = "macos"))]
            {
                let w = CAPTION_BTN_W * 3.0;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(bar.right() - w, bar.top()),
                    egui::vec2(w, bar.height()),
                );
                caption_buttons(
                    &mut ui.new_child(egui::UiBuilder::new().max_rect(rect)),
                    &ctx,
                    theme,
                );
            }
            // 标题居中（logo + 文字）
            let logo = logo_texture(&ctx);
            let ppp = ctx.pixels_per_point();
            let logo_size = logo.size_vec2() / ppp;
            let gap = 5.0;
            let title_galley = ui.painter().layout_no_wrap(
                title.to_owned(),
                egui::FontId::proportional(14.0),
                theme.accent,
            );
            let total_w = logo_size.x + gap + title_galley.size().x;
            let left_x = bar.center().x - total_w / 2.0;
            let logo_rect = egui::Rect::from_min_size(
                egui::pos2(left_x, bar.center().y - logo_size.y / 2.0 + 1.0),
                logo_size,
            );
            ui.painter().image(
                logo.id(),
                logo_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.painter().galley(
                egui::pos2(
                    left_x + logo_size.x + gap,
                    bar.center().y - title_galley.size().y / 2.0,
                ),
                title_galley,
                egui::Color32::PLACEHOLDER,
            );
        });
}

#[cfg(not(target_os = "macos"))]
fn caption_buttons(ui: &mut egui::Ui, ctx: &egui::Context, theme: &ShellTheme) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if caption_button(ui, Caption::Minimize, theme) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let icon = if maximized {
            Caption::Restore
        } else {
            Caption::Maximize
        };
        if caption_button(ui, icon, theme) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if caption_button(ui, Caption::Close, theme) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

#[cfg(not(target_os = "macos"))]
impl Caption {
    fn glyph(self) -> &'static str {
        match self {
            Self::Minimize => codicon::CHROME_MINIMIZE,
            Self::Maximize => codicon::CHROME_MAXIMIZE,
            Self::Restore => codicon::CHROME_RESTORE,
            Self::Close => codicon::CHROME_CLOSE,
        }
    }
    fn is_close(self) -> bool {
        matches!(self, Self::Close)
    }
}

#[cfg(not(target_os = "macos"))]
fn caption_button(ui: &mut egui::Ui, icon: Caption, theme: &ShellTheme) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(CAPTION_BTN_W, theme.title_bar_height),
        egui::Sense::click(),
    );
    if response.hovered() {
        let bg = if icon.is_close() {
            theme.close_hover
        } else {
            theme.caption_hover
        };
        ui.painter().rect_filled(rect, 0.0, bg);
    }
    let fg = if response.hovered() && icon.is_close() {
        egui::Color32::WHITE
    } else {
        theme.text_secondary
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon.glyph(),
        egui::FontId::new(13.0, codicon::family()),
        fg,
    );
    response.clicked()
}
