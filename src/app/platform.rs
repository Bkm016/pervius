//! 平台相关文件操作：在资源管理器中显示文件/目录
//!
//! @author sky

use super::App;
use pervius_java_bridge::decompiler;
use std::path::Path;

impl App {
    /// 在文件管理器中显示一个真实存在的文件系统路径。
    pub(crate) fn reveal_filesystem_path(&self, path: &Path) {
        if path.is_dir() {
            open_directory(path);
        } else {
            reveal_in_explorer(path);
        }
    }

    /// 处理 explorer 中右键「Reveal in Explorer」
    pub(crate) fn handle_pending_reveal(&mut self) {
        let Some(entry_path) = self.layout.file_panel.pending_reveal.take() else {
            return;
        };
        let Some(jar) = self.workspace.jar() else {
            return;
        };
        log::info!("Reveal: entry_path={entry_path}");
        // class 文件：定位到缓存的反编译源码
        if entry_path.ends_with(".class") || entry_path.contains('$') {
            if let Some(file) = decompiler::cached_source_path(&jar.hash, &entry_path) {
                log::info!("Reveal: found {}", file.display());
                self.reveal_filesystem_path(&file);
                return;
            }
            log::warn!("Reveal: cached source not found for {entry_path}");
        }
        // 缓存未命中：直接打开缓存目录
        if let Ok(dir) = decompiler::cache_dir(&jar.hash) {
            if dir.exists() {
                log::info!("Reveal: fallback to dir {}", dir.display());
                self.reveal_filesystem_path(&dir);
            }
        }
    }
}

/// 在资源管理器/Finder 中选中文件
#[cfg(windows)]
fn reveal_in_explorer(path: &Path) {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn SHParseDisplayName(
            name: *const u16,
            ctx: *const c_void,
            pidl: *mut *mut c_void,
            sfgao_in: u32,
            sfgao_out: *mut u32,
        ) -> i32;
        fn SHOpenFolderAndSelectItems(
            dir: *const c_void,
            count: u32,
            items: *const *const c_void,
            flags: u32,
        ) -> i32;
        fn CoTaskMemFree(pv: *mut c_void);
    }

    // 规范化路径分隔符（join 产生的 "/" 混入会导致 SHParseDisplayName 失败）
    let normalized: std::path::PathBuf = path.components().collect();
    let wide: Vec<u16> = normalized
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut pidl: *mut c_void = std::ptr::null_mut();
        if SHParseDisplayName(
            wide.as_ptr(),
            std::ptr::null(),
            &mut pidl,
            0,
            std::ptr::null_mut(),
        ) != 0
        {
            return;
        }
        SHOpenFolderAndSelectItems(pidl, 0, std::ptr::null(), 0);
        CoTaskMemFree(pidl);
    }
}

#[cfg(target_os = "macos")]
fn reveal_in_explorer(path: &Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
}

#[cfg(not(any(windows, target_os = "macos")))]
fn reveal_in_explorer(path: &Path) {
    // Linux: 用 xdg-open 打开父目录
    if let Some(parent) = path.parent() {
        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
    }
}

/// 直接打开目录（显示其内容）
#[cfg(windows)]
fn open_directory(path: &Path) {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *const c_void,
            op: *const u16,
            file: *const u16,
            params: *const u16,
            dir: *const u16,
            show: i32,
        ) -> isize;
    }

    let normalized: std::path::PathBuf = path.components().collect();
    let wide: Vec<u16> = normalized
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let open: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null(),
            open.as_ptr(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
    }
}

#[cfg(not(windows))]
fn open_directory(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}
