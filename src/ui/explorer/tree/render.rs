//! 文件树渲染（虚拟滚动 + 展开动画）
//!
//! 使用 ScrollArea::show_rows() 实现行级虚拟化，
//! 只渲染视口内可见的行，全展开 5000+ 节点也不掉帧。
//! 展开/折叠带行数渐变 + 箭头旋转动画。
//!
//! @author sky

use super::node::TreeNode;
use crate::appearance::{codicon, theme};
use eframe::egui;
use egui_animation::Anim;
use egui_shell::components::menu_item_raw;
use rust_i18n::t;
use std::collections::HashSet;
use std::ops::Range;

/// 根据 openness 计算有效行数
fn effective_count(full: usize, openness: f32) -> usize {
    if openness >= 1.0 {
        full
    } else {
        ((full as f32) * openness).ceil() as usize
    }
}

/// 获取节点的展开动画进度（0.0 折叠 → 1.0 展开）
///
/// 过滤模式下所有可见节点强制展开（target=true），
/// 退出过滤时自动动画回到 `node.expanded` 状态。
fn node_openness(ctx: &egui::Context, node: &TreeNode, filtering: bool) -> f32 {
    let target = filtering || node.expanded;
    ctx.animate_bool_with_time(
        egui::Id::new("tree_expand").with(&node.path),
        target,
        theme::TREE_EXPAND_DURATION,
    )
}

/// 递归计数可见节点总数（展开动画插值）
fn count_rows(ctx: &egui::Context, nodes: &[TreeNode], visible: &HashSet<String>) -> usize {
    let filtering = !visible.is_empty();
    let mut count = 0;
    for node in nodes {
        if filtering && !visible.contains(&node.path) {
            continue;
        }
        count += 1;
        if node.has_children() {
            let openness = node_openness(ctx, node, filtering);
            if openness > 0.0 {
                let full = count_rows(ctx, &node.children, visible);
                count += effective_count(full, openness);
            }
        }
    }
    count
}

/// 虚拟滚动入口：计数 → show_rows → 只渲染可见范围
pub fn render_tree(
    ui: &mut egui::Ui,
    nodes: &mut [TreeNode],
    selected: &Option<String>,
    visible: &HashSet<String>,
    reveal: &mut Option<String>,
    rename: &mut Option<String>,
    delete: &mut Option<String>,
    scroll: bool,
    tab_modified: &HashSet<String>,
    jar_modified: &HashSet<String>,
    decompiled_classes: Option<&HashSet<String>>,
) -> Option<String> {
    ui.spacing_mut().item_spacing.y = 2.0;
    let row_h = theme::TREE_ROW_HEIGHT + 2.0;
    let ctx = ui.ctx().clone();
    // 每帧只计算一次行数，避免多 pass 之间 total 不一致导致 show_rows 范围漂移
    let total = {
        let frame = ctx.cumulative_frame_nr();
        let cache_id = egui::Id::new("tree_row_count");
        match ctx.data(|d| d.get_temp::<(u64, usize)>(cache_id)) {
            Some((f, t)) if f == frame => t,
            _ => {
                let t = count_rows(&ctx, nodes, visible);
                ctx.data_mut(|d| d.insert_temp(cache_id, (frame, t)));
                t
            }
        }
    };
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_salt("file_tree")
        .auto_shrink(false)
        .min_scrolled_height(ui.available_height())
        .show_rows(ui, row_h, total, |ui, range| {
            let mut counter = 0usize;
            let (click, _) = render_range(
                ui,
                &ctx,
                nodes,
                0,
                &mut counter,
                &range,
                selected,
                visible,
                reveal,
                rename,
                delete,
                scroll,
                tab_modified,
                jar_modified,
                decompiled_classes,
                usize::MAX,
            );
            clicked = click;
        });
    clicked
}

/// 递归遍历树，只渲染 counter 落在 range 内的节点
///
/// `max_rows` 限制本次调用最多消耗的行数（用于展开动画的渐进显示），
/// 返回 (点击路径, 实际消耗行数)。
fn render_range(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    nodes: &mut [TreeNode],
    depth: u8,
    counter: &mut usize,
    range: &Range<usize>,
    selected: &Option<String>,
    visible: &HashSet<String>,
    reveal: &mut Option<String>,
    rename: &mut Option<String>,
    delete: &mut Option<String>,
    scroll: bool,
    tab_modified: &HashSet<String>,
    jar_modified: &HashSet<String>,
    decompiled_classes: Option<&HashSet<String>>,
    max_rows: usize,
) -> (Option<String>, usize) {
    let filtering = !visible.is_empty();
    let mut clicked = None;
    let mut consumed = 0;
    for node in nodes.iter_mut() {
        if consumed >= max_rows {
            break;
        }
        if filtering && !visible.contains(&node.path) {
            continue;
        }
        let idx = *counter;
        *counter += 1;
        consumed += 1;
        // 已超过可见范围，后续节点全部跳过
        if idx >= range.end {
            return (clicked, consumed);
        }
        // 在可见范围内才渲染
        if idx >= range.start {
            let is_selected = selected.as_ref().is_some_and(|s| s == &node.path);
            let mod_color = if tab_modified.contains(&node.path) {
                Some(theme::ACCENT_ORANGE)
            } else if jar_modified.contains(&node.path) {
                Some(theme::ACCENT_GREEN)
            } else {
                None
            };
            let openness = if node.is_folder || node.has_children() {
                node_openness(ctx, node, filtering)
            } else {
                0.0
            };
            let (single, double) = render_row(
                ui,
                node,
                idx,
                depth,
                is_selected,
                mod_color,
                decompiled_classes,
                reveal,
                rename,
                delete,
                scroll,
                openness,
            );
            if node.is_folder {
                if single {
                    node.expanded = !node.expanded;
                }
            } else if node.label.starts_with('$') {
                if double {
                    node.expanded = !node.expanded;
                }
            } else {
                if single {
                    clicked = Some(node.path.clone());
                }
                if double && node.has_children() {
                    node.expanded = !node.expanded;
                }
            }
        }
        // 子节点递归
        if node.has_children() {
            let openness = node_openness(ctx, node, filtering);
            if openness > 0.0 {
                let full = count_rows(ctx, &node.children, visible);
                let effective = effective_count(full, openness);
                let budget = (max_rows - consumed).min(effective);
                let (path, child_consumed) = render_range(
                    ui,
                    ctx,
                    &mut node.children,
                    depth + 1,
                    counter,
                    range,
                    selected,
                    visible,
                    reveal,
                    rename,
                    delete,
                    scroll,
                    tab_modified,
                    jar_modified,
                    decompiled_classes,
                    budget,
                );
                // 快进未访问的行以保持 counter 与 count_rows 一致
                *counter += budget - child_consumed;
                consumed += budget;
                if path.is_some() {
                    clicked = path;
                }
            }
        }
    }
    (clicked, consumed)
}

/// 判断节点是否已反编译
///
/// `decompiled_classes` 为 None 表示全部已完成，Some 则逐个检查：
/// - 根节点（path 为空）：始终视为已完成
/// - 文件夹：路径在集合中（反编译线程写入类名时同时写入文件夹前缀）
/// - class 文件：去掉 `.class` 后缀和内部类 `$` 后缀，检查外层类是否在集合中
fn is_node_decompiled(node: &TreeNode, decompiled_classes: Option<&HashSet<String>>) -> bool {
    let set = tabookit::or!(decompiled_classes, return true);
    if node.path.is_empty() {
        return true;
    }
    if node.is_folder {
        return set.contains(&node.path);
    }
    let without_ext = node.path.strip_suffix(".class").unwrap_or(&node.path);
    let base = match without_ext.find('$') {
        Some(pos) => &without_ext[..pos],
        None => without_ext,
    };
    set.contains(base)
}

/// 渲染一行树节点，返回 (单击, 双击)
fn render_row(
    ui: &mut egui::Ui,
    node: &TreeNode,
    row_idx: usize,
    depth: u8,
    selected: bool,
    mod_color: Option<egui::Color32>,
    decompiled_classes: Option<&HashSet<String>>,
    reveal: &mut Option<String>,
    rename: &mut Option<String>,
    delete: &mut Option<String>,
    scroll: bool,
    openness: f32,
) -> (bool, bool) {
    let indent_px = 8.0 + depth as f32 * 16.0;
    let avail_w = ui.available_width();
    // 使用行索引作为位置稳定 ID，避免过滤切换内容时同一 rect 的 ID 在帧间漂移
    let (_, rect) = ui.allocate_space(egui::vec2(avail_w, theme::TREE_ROW_HEIGHT));
    let response = ui.interact(
        rect,
        egui::Id::new("tree_row").with(row_idx),
        egui::Sense::click(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    // 选中 / hover 背景动画
    let anim = Anim::new(ui, 0.2).with(&node.path);
    let will_select = !node.is_folder && !node.label.starts_with('$') && response.clicked();
    let bg = anim.select_bg(
        selected,
        response.hovered(),
        will_select,
        theme::verdigris_alpha(38),
        theme::BG_HOVER,
    );
    if bg.a() > 0 {
        painter.rect_filled(rect, 4.0, bg);
    }
    if selected && scroll {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    let y = rect.center().y;
    let mut x = rect.left() + indent_px;
    // 折叠箭头（旋转三角形，跟随 openness 平滑旋转）
    if node.is_folder || node.has_children() {
        let center = egui::pos2(x + 6.0, y);
        let rot = egui::emath::Rot2::from_angle(std::f32::consts::FRAC_PI_2 * openness);
        // 向右的三角形顶点（相对于中心），旋转后变为向下
        let a = center + rot * egui::vec2(-1.5, -2.5);
        let b = center + rot * egui::vec2(2.5, 0.0);
        let c = center + rot * egui::vec2(-1.5, 2.5);
        painter.add(egui::Shape::convex_polygon(
            vec![a, b, c],
            theme::TEXT_MUTED,
            egui::Stroke::NONE,
        ));
    }
    x += 12.0 + 6.0;
    // 按节点判断是否已反编译，未完成的暗显，完成后动画过渡到正常色
    let node_decompiled = is_node_decompiled(node, decompiled_classes);
    let decompile_anim = Anim::new(ui, 0.35).with(&node.path);
    let target_icon = if node_decompiled {
        node.icon_color()
    } else {
        theme::TEXT_MUTED
    };
    let icon_color = decompile_anim.color("icon", target_icon);
    let target_label = if let Some(c) = mod_color {
        c
    } else if node_decompiled {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_MUTED
    };
    let label_color = decompile_anim.color("label", target_label);
    // 图标
    painter.text(
        egui::pos2(x + 8.0, y),
        egui::Align2::CENTER_CENTER,
        node.icon(),
        egui::FontId::new(14.0, codicon::family()),
        icon_color,
    );
    x += 16.0 + 6.0;
    // 名称
    painter.text(
        egui::pos2(x, y),
        egui::Align2::LEFT_CENTER,
        &node.label,
        egui::FontId::proportional(12.0),
        label_color,
    );
    // 右键菜单
    if !node.is_folder {
        response.context_menu(|ui| {
            ui.style_mut().visuals.widgets.hovered.bg_fill = theme::BG_HOVER;
            if menu_item_raw(ui, &theme::menu_theme(), &t!("explorer.reveal"), "") {
                *reveal = Some(node.path.clone());
                ui.close();
            }
            if menu_item_raw(ui, &theme::menu_theme(), &t!("explorer.rename"), "") {
                *rename = Some(node.path.clone());
                ui.close();
            }
            ui.separator();
            if menu_item_raw(ui, &theme::menu_theme(), &t!("explorer.delete"), "") {
                *delete = Some(node.path.clone());
                ui.close();
            }
        });
    }
    (response.clicked(), response.double_clicked())
}
