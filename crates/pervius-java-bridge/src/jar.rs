//! JAR/ZIP 归档读写
//!
//! @author sky

use crate::decompiler::CachedSource;
use crate::error::BridgeError;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

tabookit::class! {
    /// JAR 加载进度（跨线程共享）
    pub struct LoadProgress {
        /// 当前已处理数
        pub current: AtomicU32,
        /// 总数
        pub total: AtomicU32,
    }

    pub fn new() -> Self {
        Self {
            current: AtomicU32::new(0),
            total: AtomicU32::new(0),
        }
    }
}

/// 已修改条目：修改后的字节 + 反编译缓存
pub struct ModifiedEntry {
    /// 修改后的 class/文件字节
    pub data: Vec<u8>,
    /// 反编译缓存（反编译完成后填入）
    pub decompiled: Option<CachedSource>,
}

tabookit::class! {
    /// JAR 归档内存表示
    pub struct JarArchive {
        /// 归档文件名
        pub name: String,
        /// 原始文件路径
        pub path: PathBuf,
        /// 文件内容 SHA-256（hex，前 16 字符用于缓存目录）
        pub hash: String,
        /// 原始文件大小（字节）
        pub file_size: u64,
        /// 条目路径 → 原始字节（不可变）
        entries: HashMap<String, Vec<u8>>,
        /// 已修改条目（路径 → 修改后的数据 + 反编译缓存）
        modified_entries: HashMap<String, ModifiedEntry>,
    }

    /// 带进度回报的打开方法（供后台线程调用）
    pub fn open_with_progress(path: &Path, progress: &LoadProgress) -> Result<Self, BridgeError> {
        let data = std::fs::read(path)?;
        let file_size = data.len() as u64;
        let hash: String = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let cursor = std::io::Cursor::new(data);
        let mut zip = zip::ZipArchive::new(cursor).map_err(BridgeError::parse)?;
        let total = zip.len();
        progress.total.store(total as u32, Ordering::Relaxed);
        let mut entries = HashMap::new();
        for i in 0..total {
            let mut entry = zip.by_index(i).map_err(BridgeError::parse)?;
            if !entry.is_dir() {
                let name = entry.name().to_owned();
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf)?;
                entries.insert(name, buf);
            }
            progress.current.store((i + 1) as u32, Ordering::Relaxed);
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Self {
            name,
            path: path.to_path_buf(),
            hash,
            file_size,
            entries,
            modified_entries: HashMap::new(),
        })
    }

    /// 获取条目内容（已修改条目返回修改后的数据）
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        if let Some(m) = self.modified_entries.get(path) {
            return Some(&m.data);
        }
        self.entries.get(path).map(|v| v.as_slice())
    }

    /// 更新条目内容（写入已修改区，保留原始数据不动）
    pub fn put(&mut self, path: &str, data: Vec<u8>) {
        match self.modified_entries.get_mut(path) {
            Some(m) => {
                m.data = data;
                m.decompiled = None;
            }
            None => {
                self.modified_entries.insert(
                    path.to_string(),
                    ModifiedEntry {
                        data,
                        decompiled: None,
                    },
                );
            }
        }
    }

    /// 是否有任何已修改条目
    pub fn has_modified_entries(&self) -> bool {
        !self.modified_entries.is_empty()
    }

    /// 清除所有已修改条目（放弃变更时调用，恢复到原始数据）
    pub fn clear_modified(&mut self) {
        self.modified_entries.clear();
    }

    /// 将已修改条目提交为新的内存基线（覆盖源 JAR 成功后调用）。
    pub fn commit_modified_from_file(&mut self, path: &Path) -> Result<(), BridgeError> {
        for (entry_path, modified) in self.modified_entries.drain() {
            self.entries.insert(entry_path, modified.data);
        }
        let data = std::fs::read(path)?;
        self.file_size = data.len() as u64;
        self.hash = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(())
    }

    /// 条目是否已修改
    pub fn is_modified(&self, path: &str) -> bool {
        self.modified_entries.contains_key(path)
    }

    /// 已修改条目路径迭代器
    pub fn modified_paths(&self) -> impl Iterator<Item = &str> {
        self.modified_entries.keys().map(|s| s.as_str())
    }

    /// 已修改条目数量
    pub fn modified_count(&self) -> usize {
        self.modified_entries.len()
    }

    /// 缓存已修改条目的反编译结果
    pub fn put_decompiled(&mut self, path: &str, cached: CachedSource) {
        if let Some(m) = self.modified_entries.get_mut(path) {
            m.decompiled = Some(cached);
        }
    }

    /// 获取已修改条目的反编译缓存
    pub fn get_decompiled(&self, path: &str) -> Option<&CachedSource> {
        self.modified_entries.get(path)?.decompiled.as_ref()
    }

    /// 获取排序后的条目路径列表
    pub fn paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.entries.keys().map(|s| s.as_str()).collect();
        for path in self.modified_entries.keys().map(|s| s.as_str()) {
            if !self.entries.contains_key(path) {
                paths.push(path);
            }
        }
        paths.sort_unstable();
        paths
    }

    /// .class 文件条目数量
    pub fn class_count(&self) -> u32 {
        self.paths()
            .into_iter()
            .filter(|k| k.ends_with(".class"))
            .count() as u32
    }

    /// 快照所有条目（已修改条目使用修改后的字节）
    ///
    /// 返回排序后的 `(路径, 字节)` 列表，可安全移入后台线程。
    pub fn snapshot_entries(&self) -> Vec<(String, Vec<u8>)> {
        let mut paths: Vec<String> = self.entries.keys().cloned().collect();
        for path in self.modified_entries.keys() {
            if !self.entries.contains_key(path) {
                paths.push(path.clone());
            }
        }
        paths.sort_unstable();
        paths
            .into_iter()
            .map(|p| {
                let data = self.get(&p).unwrap().to_vec();
                (p, data)
            })
            .collect()
    }
}

/// 将条目快照写成 JAR 文件
///
/// 独立函数，可在后台线程中调用（不持有 `JarArchive` 引用）。
pub fn write_jar(
    entries: &[(String, Vec<u8>)],
    output: &Path,
    progress: &LoadProgress,
) -> Result<usize, BridgeError> {
    let file = std::fs::File::create(output)?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    progress
        .total
        .store(entries.len() as u32, Ordering::Relaxed);
    for (i, (path, data)) in entries.iter().enumerate() {
        writer
            .start_file(path, options)
            .map_err(BridgeError::parse)?;
        writer.write_all(data)?;
        progress.current.store((i + 1) as u32, Ordering::Relaxed);
    }
    writer.finish().map_err(BridgeError::parse)?;
    Ok(entries.len())
}
