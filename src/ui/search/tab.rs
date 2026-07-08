//! 搜索浮动面板：egui::Window 承载，可拖动、可 resize、不阻挡后方交互
//!
//! 反编译源码全文搜索，结果列表 + 底部预览编辑器。
//! 索引和搜索均在后台线程执行，UI 线程零阻塞。
//!
//! @author sky

use super::index::{self, MAX_MATCHES, SearchIndex, SearchMessage};
use super::result::SearchResultGroup;
use super::widget::{self, render_group_header, separator};
use crate::appearance::theme::flat_button_theme;
use crate::appearance::{codicon, theme};
use eframe::egui;
use egui_editor::LayoutCache;
use egui_editor::highlight::{self, Span};
use egui_editor::search::FindMatch;
use egui_shell::components::{FlatButton, FloatingWindow};
use rust_i18n::t;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 搜索输入防抖延迟（毫秒）
const DEBOUNCE_MS: u64 = 200;

/// 搜索结果打开请求
pub struct SearchOpenRequest {
    /// JAR 内条目路径
    pub entry_path: String,
    /// 目标行号（0-based）
    pub line: usize,
}

/// 预览面板缓存：源码文本 + 语法高亮 span + code_view 布局缓存
struct PreviewCache {
    /// 缓存键 (group_index, match_index)
    key: (usize, usize),
    /// 预览源码文本（从索引 clone）
    source: String,
    /// 语法高亮 span
    spans: Vec<Span>,
    /// 匹配行在源码中的字节区间（用于搜索高亮）
    find_matches: Vec<FindMatch>,
    /// code_view 内部布局缓存
    layout_cache: Option<LayoutCache>,
}

tabookit::class! {
    /// 搜索浮动面板
    pub struct SearchDialog {
        /// 浮动窗口（可拖动、可 resize）
        window: FloatingWindow,
        /// 用户输入的搜索文本
        query: String,
        /// 上一帧的搜索文本（变更检测用）
        prev_query: String,
        /// 当前搜索结果（按类分组）
        results: Vec<SearchResultGroup>,
        /// 下一帧是否聚焦搜索输入框
        focus_input: bool,
        /// 是否区分大小写
        case_sensitive: bool,
        /// 是否启用正则表达式
        use_regex: bool,
        /// 当前选中的匹配项 (group_index, match_index)
        selected: Option<(usize, usize)>,
        /// 是否需要滚动预览面板到选中匹配行
        scroll_to_match: bool,
        /// 预览面板语法高亮缓存
        preview_cache: Option<PreviewCache>,
        /// 搜索索引（从 App 接收）
        search_index: Option<Arc<SearchIndex>>,
        /// 后台搜索流式接收端
        search_rx: Option<mpsc::Receiver<SearchMessage>>,
        /// 后台搜索取消标志（Drop 或新搜索时置 true）
        search_cancel: Option<Arc<AtomicBool>>,
        /// 上次搜索结果的统计信息
        result_summary: Option<ResultSummary>,
        /// 待打开的搜索结果
        pending_open: Option<SearchOpenRequest>,
        /// 是否有 JAR 打开（用于区分提示信息）
        has_jar: bool,
        /// 索引是否正在构建中
        index_building: bool,
        /// 搜索结果列表高度占比（可拖拽调整）
        results_ratio: f32,
        /// 搜索输入防抖截止时间
        debounce_deadline: Option<Instant>,
    }

    pub fn new() -> Self {
        Self {
            window: FloatingWindow::new("search_window", t!("search.title").to_string())
                .icon(codicon::SEARCH)
                .default_size([700.0, 520.0])
                .min_size([400.0, 300.0]),
            query: String::new(),
            prev_query: String::new(),
            results: Vec::new(),
            focus_input: false,
            case_sensitive: false,
            use_regex: false,
            selected: None,
            scroll_to_match: false,
            preview_cache: None,
            search_index: None,
            search_rx: None,
            search_cancel: None,
            result_summary: None,
            pending_open: None,
            has_jar: false,
            index_building: false,
            results_ratio: 0.42,
            debounce_deadline: None,
        }
    }

    pub fn open(&mut self) {
        if self.window.is_open() {
            return;
        }
        if self.search_index.is_none() {
            return;
        }
        self.window.open();
        self.focus_input = true;
        self.query.clear();
        self.prev_query.clear();
        self.results.clear();
        self.selected = None;
        self.result_summary = None;
        self.preview_cache = None;
        self.debounce_deadline = None;
        // 取消旧搜索
        if let Some(c) = self.search_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.search_rx = None;
    }

    /// 用指定查询文本打开搜索面板并立即搜索（Find Usages 用）
    pub fn open_with_query(&mut self, query: &str) {
        if self.search_index.is_none() {
            return;
        }
        // 取消旧搜索
        if let Some(c) = self.search_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.search_rx = None;
        self.window.open();
        self.focus_input = false;
        self.query = query.to_string();
        self.prev_query = query.to_string();
        self.results.clear();
        self.selected = None;
        self.result_summary = None;
        self.preview_cache = None;
        self.debounce_deadline = None;
        self.dispatch_search();
    }

    /// 同步搜索状态（App 每帧调用）
    pub fn sync_state(
        &mut self,
        index: Option<Arc<SearchIndex>>,
        has_jar: bool,
        index_building: bool,
    ) {
        self.search_index = index;
        self.has_jar = has_jar;
        self.index_building = index_building;
    }

    /// 取出待打开的搜索结果请求
    pub fn take_pending_open(&mut self) -> Option<SearchOpenRequest> {
        self.pending_open.take()
    }

    pub fn render(&mut self, ctx: &egui::Context, shell_theme: &egui_shell::ShellTheme) {
        self.poll_search_result(ctx);
        self.poll_debounce(ctx);
        let mut window = std::mem::take(&mut self.window);
        let summary = self.result_summary;
        let searching = self.search_rx.is_some();
        let live_matches: usize = self.results.iter().map(|g| g.matches.len()).sum();
        let live_files = self.results.len();
        window.show(
            ctx,
            shell_theme,
            |ui| {
                // header 右侧：搜索统计
                if let Some(s) = summary {
                    ui.label(
                        egui::RichText::new(t!(
                            "search.summary",
                            matches = s.total_matches,
                            files = s.files_matched
                        ))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                    );
                    if s.truncated {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(t!("search.truncated", max = MAX_MATCHES))
                                .size(11.0)
                                .color(theme::ACCENT_ORANGE),
                        );
                    }
                } else if searching && live_matches > 0 {
                    ui.label(
                        egui::RichText::new(t!(
                            "search.summary",
                            matches = live_matches,
                            files = live_files
                        ))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                    );
                }
            },
            |ui| {
                self.render_toolbar(ui);
                separator(ui);
                // 输入变化时启动防抖定时器，不立即搜索
                if self.query != self.prev_query {
                    self.prev_query = self.query.clone();
                    self.debounce_deadline =
                        Some(Instant::now() + Duration::from_millis(DEBOUNCE_MS));
                    ui.ctx()
                        .request_repaint_after(Duration::from_millis(DEBOUNCE_MS));
                }
                self.render_body(ui);
            },
        );
        self.window = window;
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        let fbt = flat_button_theme(theme::TEXT_SECONDARY);
        let toolbar_h = 36.0;
        let avail_w = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(avail_w, toolbar_h), egui::Sense::hover());
        let mut tb = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        tb.add_space(8.0);
        tb.label(
            egui::RichText::new(codicon::SEARCH)
                .font(egui::FontId::new(14.0, codicon::family()))
                .color(theme::TEXT_MUTED),
        );
        tb.add_space(4.0);
        let input_width = (tb.available_width() - 80.0).max(120.0);
        let resp = tb.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text(t!("search.hint"))
                .frame(egui::Frame::NONE)
                .text_color(theme::TEXT_PRIMARY)
                .font(egui::FontId::proportional(13.0))
                .desired_width(input_width),
        );
        if self.focus_input {
            resp.request_focus();
            self.focus_input = false;
        }
        tb.add_space(4.0);
        let btn_case = FlatButton::new(codicon::CASE_SENSITIVE, &fbt)
            .font_family(codicon::family())
            .font_size(15.0)
            .active(self.case_sensitive)
            .inactive_color(theme::TEXT_MUTED)
            .min_size(egui::vec2(28.0, 24.0));
        if tb
            .add(btn_case)
            .on_hover_text(t!("search.match_case"))
            .clicked()
        {
            self.case_sensitive = !self.case_sensitive;
            self.debounce_deadline = None;
            self.prev_query = self.query.clone();
            self.dispatch_search();
        }
        tb.add_space(2.0);
        let btn_regex = FlatButton::new(codicon::REGEX, &fbt)
            .font_family(codicon::family())
            .font_size(15.0)
            .active(self.use_regex)
            .inactive_color(theme::TEXT_MUTED)
            .min_size(egui::vec2(28.0, 24.0));
        if tb
            .add(btn_regex)
            .on_hover_text(t!("search.use_regex"))
            .clicked()
        {
            self.use_regex = !self.use_regex;
            self.debounce_deadline = None;
            self.prev_query = self.query.clone();
            self.dispatch_search();
        }
    }

    fn render_body(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let placeholder = if self.search_index.is_none() {
            Some(if !self.has_jar {
                t!("search.no_jar")
            } else if self.index_building {
                t!("search.building_index")
            } else {
                t!("search.type_to_search")
            })
        } else if self.query.is_empty() {
            Some(t!("search.type_to_search"))
        } else if self.results.is_empty() {
            // 搜索进行中时不显示 "无结果"，等搜索完成再判断
            Some(if self.search_rx.is_some() {
                t!("search.searching")
            } else {
                t!("search.no_results")
            })
        } else {
            None
        };
        if let Some(msg) = placeholder {
            // 用 painter 绘制占位文字，不注册 widget，避免状态切换时产生 ID 冲突警告
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                egui::FontId::proportional(12.0),
                theme::TEXT_MUTED,
            );
            return;
        }
        let avail_h = rect.height();
        let list_h =
            (avail_h * self.results_ratio).clamp(theme::SEARCH_MIN_RESULTS_H, avail_h - theme::SEARCH_MIN_PREVIEW_H);
        // 结果列表
        let list_rect =
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(rect.width(), list_h));
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
        self.render_results(&mut list_ui);
        // 拖拽 resize 手柄
        let divider_y = rect.top() + list_h;
        let handle_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, divider_y),
            egui::vec2(rect.width(), theme::SEARCH_RESIZE_HANDLE_H),
        );
        let handle_id = ui.id().with("search_split_resize");
        let handle_resp = ui.interact(handle_rect, handle_id, egui::Sense::drag());
        if handle_resp.dragged() {
            let new_h = (list_h + handle_resp.drag_delta().y)
                .clamp(theme::SEARCH_MIN_RESULTS_H, avail_h - theme::SEARCH_MIN_PREVIEW_H);
            self.results_ratio = new_h / avail_h;
        }
        if handle_resp.hovered() || handle_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
        }
        // 分割线
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), divider_y),
                egui::pos2(rect.right(), divider_y),
            ],
            egui::Stroke::new(1.0, theme::BORDER),
        );
        // 预览面板
        let preview_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), divider_y + 1.0),
            rect.right_bottom(),
        );
        let mut preview_ui = ui.new_child(egui::UiBuilder::new().max_rect(preview_rect));
        self.render_preview(&mut preview_ui);
        ui.allocate_rect(rect, egui::Sense::hover());
    }

    fn render_results(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("search_results")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(2.0);
                let mut toggle_idx: Option<usize> = None;
                let mut click: Option<(usize, usize)> = None;
                let mut dbl_click: Option<(usize, usize)> = None;
                for (gi, group) in self.results.iter().enumerate() {
                    ui.push_id(gi, |ui| {
                        if render_group_header(ui, group) {
                            toggle_idx = Some(gi);
                        }
                        if group.expanded {
                            for (mi, m) in group.matches.iter().enumerate() {
                                ui.push_id(mi, |ui| {
                                    let sel = self.selected == Some((gi, mi));
                                    let avail_w = ui.available_width();
                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(avail_w, theme::SEARCH_ROW_HEIGHT),
                                        egui::Sense::click(),
                                    );
                                    let painter = ui.painter();
                                    if sel {
                                        painter.rect_filled(rect, 0.0, theme::BG_HOVER);
                                    } else if resp.hovered() {
                                        painter.rect_filled(rect, 0.0, theme::BG_LIGHT);
                                    }
                                    let mid_y = rect.center().y;
                                    let line_label = format!("{}", m.line + 1);
                                    painter.text(
                                        egui::pos2(rect.left() + 48.0, mid_y),
                                        egui::Align2::RIGHT_CENTER,
                                        &line_label,
                                        egui::FontId::monospace(11.0),
                                        theme::TEXT_MUTED,
                                    );
                                    let preview_x = rect.left() + 56.0;
                                    let job = widget::highlight_preview(&m.preview, &m.highlights);
                                    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                                    painter.galley(
                                        egui::pos2(preview_x, mid_y - galley.size().y / 2.0),
                                        galley,
                                        egui::Color32::PLACEHOLDER,
                                    );
                                    if resp.double_clicked() {
                                        dbl_click = Some((gi, mi));
                                    } else if resp.clicked() {
                                        click = Some((gi, mi));
                                    }
                                });
                            }
                        }
                    });
                }
                if let Some(i) = toggle_idx {
                    self.results[i].expanded = !self.results[i].expanded;
                }
                if let Some(key) = dbl_click.or(click) {
                    if self.selected != Some(key) {
                        self.selected = Some(key);
                        self.scroll_to_match = true;
                    }
                }
                // 双击打开文件
                if let Some((gi, mi)) = dbl_click {
                    if let Some(group) = self.results.get(gi) {
                        if let Some(m) = group.matches.get(mi) {
                            self.pending_open = Some(SearchOpenRequest {
                                entry_path: group.entry_path.clone(),
                                line: m.line,
                            });
                        }
                    }
                }
                ui.add_space(4.0);
            });
    }

    fn render_preview(&mut self, ui: &mut egui::Ui) {
        let (gi, mi) = tabookit::or!(self.selected, {
            let rect = ui.available_rect_before_wrap();
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t!("search.select_preview"),
                egui::FontId::proportional(12.0),
                theme::TEXT_MUTED,
            );
            return;
        });
        let group = tabookit::or!(self.results.get(gi), return);
        let m = tabookit::or!(group.matches.get(mi), return);
        let index = tabookit::or!(&self.search_index, return);
        let entry = tabookit::or!(index.entries.get(group.source_index), return);
        let match_line = m.line;
        // 缓存失效时重建预览数据
        let cache_key = (gi, mi);
        let need_recompute = self
            .preview_cache
            .as_ref()
            .is_none_or(|c| c.key != cache_key);
        if need_recompute {
            let source = entry.source.clone();
            let lang = highlight::Language::Java;
            let spans = highlight::compute_spans(&source, lang);
            let find_matches = widget::compute_find_matches(&source, match_line, &m.highlights);
            self.preview_cache = Some(PreviewCache {
                key: cache_key,
                source,
                spans,
                find_matches,
                layout_cache: None,
            });
        }
        let cache = self.preview_cache.as_mut().expect("just computed");
        let t = theme::editor_theme();
        let line_count = cache.source.split('\n').count().max(1);
        let gutter_w = egui_editor::code_view::line_number_width(line_count);
        let full_rect = ui.available_rect_before_wrap();
        egui_editor::code_view::paint_editor_bg(ui, full_rect, gutter_w, &t);
        // ScrollArea 包裹 code_view，使预览面板可滚动
        // 使用固定 id_salt，选中变更时通过 vertical_scroll_offset 定位到匹配行
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("search_preview");
        if self.scroll_to_match {
            let line_h = ui.fonts_mut(|f| {
                f.layout_no_wrap(
                    "M".to_string(),
                    egui::FontId::monospace(t.code_font_size),
                    egui::Color32::WHITE,
                )
                .size()
                .y
            });
            let target_y = match_line as f32 * line_h;
            let avail_h = full_rect.height();
            let offset = (target_y - avail_h * 0.3).max(0.0);
            scroll = scroll.vertical_scroll_offset(offset);
            self.scroll_to_match = false;
        }
        let mut hl_line = if need_recompute { Some(match_line) } else { None };
        scroll.show(ui, |ui| {
            egui_editor::code_view::code_view(
                ui,
                egui::Id::new("search_preview_cv"),
                &cache.source,
                &cache.spans,
                &[],
                &cache.find_matches,
                Some(0),
                &t,
                &mut cache.layout_cache,
                &mut hl_line,
                None,
            );
        });
    }

    /// 防抖定时器轮询（每帧 render 前调用）
    fn poll_debounce(&mut self, ctx: &egui::Context) {
        let deadline = tabookit::or!(self.debounce_deadline, return);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            self.debounce_deadline = None;
            self.dispatch_search();
        } else {
            ctx.request_repaint_after(remaining);
        }
    }

    /// 派发搜索请求到后台线程
    fn dispatch_search(&mut self) {
        // 取消旧搜索
        if let Some(c) = self.search_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.search_rx = None;
        self.results.clear();
        self.result_summary = None;
        self.selected = None;
        self.preview_cache = None;
        if self.query.is_empty() {
            return;
        }
        let index = tabookit::or!(self.search_index.clone(), return);
        let query = self.query.clone();
        let case_sensitive = self.case_sensitive;
        let use_regex = self.use_regex;
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        std::thread::spawn(move || {
            index::search_streaming(
                &index,
                &query,
                case_sensitive,
                use_regex,
                MAX_MATCHES,
                &cancel_clone,
                &tx,
            );
        });
        self.search_rx = Some(rx);
        self.search_cancel = Some(cancel);
    }

    /// 轮询搜索结果（每帧调用，限量 drain channel 增量追加）
    fn poll_search_result(&mut self, ctx: &egui::Context) {
        let rx = tabookit::or!(&self.search_rx, return);
        let mut consumed = 0;
        const BATCH: usize = 5;
        loop {
            match rx.try_recv() {
                Ok(SearchMessage::Group(group)) => {
                    self.results.push(group);
                    consumed += 1;
                    if consumed >= BATCH {
                        break;
                    }
                }
                Ok(SearchMessage::Done(done)) => {
                    self.result_summary = Some(ResultSummary {
                        total_matches: done.total_matches,
                        files_matched: done.files_matched,
                        truncated: done.truncated,
                    });
                    self.search_rx = None;
                    self.search_cancel = None;
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.search_rx = None;
                    self.search_cancel = None;
                    return;
                }
            }
        }
        // 搜索仍在进行中，持续请求重绘以便下一帧继续 drain
        ctx.request_repaint();
    }
}

/// 搜索结果统计摘要
#[derive(Clone, Copy)]
struct ResultSummary {
    /// 总匹配行数
    total_matches: usize,
    /// 有匹配的文件数
    files_matched: usize,
    /// 是否因超过上限而截断
    truncated: bool,
}
