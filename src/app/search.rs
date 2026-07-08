//! 搜索索引构建与生命周期管理
//!
//! @author sky

use super::App;
use super::workspace::Workspace;
use crate::task::{Poll, Pollable, Task};
use crate::ui::search::index::{self, IndexBuildRequest};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

impl App {
    /// 轮询搜索索引构建结果（每帧调用）
    pub(crate) fn poll_search_index(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        let Some(task) = &loaded.search_index_task else {
            return;
        };
        let result = match task.poll() {
            Poll::Ready(v) => v,
            Poll::Pending => return,
            Poll::Lost => {
                loaded.search_index_task = None;
                return;
            }
        };
        loaded.search_index_task = None;
        loaded.search_index_progress = None;
        let count = result.entries.len();
        loaded.search_index = Some(Arc::new(result));
        log::info!("Search index built: {count} classes");
    }

    #[allow(dead_code)]
    /// 标记搜索索引需要重建
    pub(crate) fn invalidate_search_index(&mut self) {
        if let Workspace::Loaded(loaded) = &mut self.workspace {
            loaded.search_index = None;
        }
    }

    /// 在后台线程构建搜索索引（搜索对话框打开且没有索引时调用）
    pub(crate) fn rebuild_search_index(&mut self) {
        let Workspace::Loaded(loaded) = &mut self.workspace else {
            return;
        };
        // 已有索引或正在构建时跳过
        if loaded.search_index.is_some() || loaded.search_index_task.is_some() {
            return;
        }
        let hash = loaded.jar.hash.clone();
        let class_paths: Vec<String> = loaded
            .jar
            .paths()
            .into_iter()
            .filter(|p| p.ends_with(".class"))
            .map(|p| p.to_string())
            .collect();
        // 预先 clone 已修改条目的反编译缓存
        let mut modified_sources = HashMap::new();
        for path in &class_paths {
            if loaded.jar.is_modified(path) {
                if let Some(cached) = loaded.jar.get_decompiled(path) {
                    modified_sources.insert(path.clone(), cached.source.clone());
                }
            }
        }
        let total = class_paths.len() as u32;
        let req = IndexBuildRequest {
            hash,
            class_paths,
            modified_sources,
        };
        let progress = Arc::new(AtomicU32::new(0));
        let p = progress.clone();
        loaded.search_index_task = Some(Task::spawn(move || index::build_index(req, &p)));
        loaded.search_index_progress = Some(progress);
        loaded.search_index_total = total;
        log::info!("Search index build started");
    }
}
