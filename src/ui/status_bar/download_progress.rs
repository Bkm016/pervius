//! 外部工具下载进度状态栏 item
//!
//! @author TheFloodDragon

use crate::appearance::theme;
use egui_shell::components::panel::status_bar::{Alignment, ProgressItem};
use pervius_java_bridge::environment::DownloadProgressSnapshot;
use rust_i18n::t;

egui_shell::define_progress_item! {
    /// 外部工具下载进度 item（左侧显示，下载期间可见）
    pub struct DownloadProgressItem;
}

impl DownloadProgressItem {
    /// 创建下载进度 item
    pub fn new() -> Self {
        Self::from_progress(ProgressItem::new(theme::TEXT_MUTED, Alignment::Left))
    }

    /// 更新下载进度，None 表示无下载任务
    pub fn set_progress(&mut self, progress: Option<DownloadProgressSnapshot>) {
        match progress {
            Some(progress) => {
                let downloaded = format_bytes(progress.downloaded);
                let text = match progress.total {
                    Some(total) if total > 0 => t!(
                        "status.downloading_known_total",
                        name = progress.file_name.as_str(),
                        downloaded = downloaded,
                        total = format_bytes(total)
                    )
                    .to_string(),
                    _ => t!(
                        "status.downloading",
                        name = progress.file_name.as_str(),
                        downloaded = downloaded
                    )
                    .to_string(),
                };
                self.set_text(text);
                self.set_visible(true);
            }
            None => self.set_visible(false),
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
