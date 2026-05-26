//! 反编译进度状态栏 item
//!
//! @author sky

use crate::appearance::theme;
use egui_shell::components::panel::status_bar::{Alignment, ProgressItem};
use rust_i18n::t;

egui_shell::define_progress_item! {
    /// 反编译进度 item（左侧显示，反编译期间可见）
    pub struct DecompileProgressItem;
}

impl DecompileProgressItem {
    /// 创建反编译进度 item
    pub fn new() -> Self {
        Self::from_progress(ProgressItem::new(theme::TEXT_MUTED, Alignment::Left))
    }

    /// 设置单文件反编译状态
    pub fn set_single(&mut self, name: &str) {
        self.set_text(t!("status.decompiling_single", name = name).to_string());
        self.set_visible(true);
    }

    /// 设置源码编译状态
    pub fn set_compile(&mut self, name: &str) {
        self.set_text(t!("status.compiling_single", name = name).to_string());
        self.set_visible(true);
    }

    /// 更新批量反编译进度，None 表示无任务
    pub fn set_progress(&mut self, info: Option<(&str, u32, u32)>) {
        match info {
            Some((name, current, total)) => {
                if total == 0 {
                    self.set_text(
                        t!("status.decompiling_init", name = name, count = 0).to_string(),
                    );
                } else {
                    self.set_text(
                        t!(
                            "status.decompiling_progress",
                            name = name,
                            current = current,
                            total = total
                        )
                        .to_string(),
                    );
                }
                self.set_visible(true);
            }
            None => {
                self.set_visible(false);
            }
        }
    }
}
