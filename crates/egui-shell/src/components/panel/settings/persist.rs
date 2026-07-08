//! TOML 配置文件持久化：`{config_dir}/{app_name}/settings.toml`
//!
//! 实现 [`SettingsFile`] trait 后自动获得 `load()` / `save()` 能力，
//! 文件不存在或格式错误时静默回退到 `Default`。
//!
//! @author sky

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::path::PathBuf;

/// TOML 配置文件持久化 trait
///
/// 实现者只需提供 `app_name()`，即可获得完整的 load/save 流程。
/// 配置文件位于 `{config_dir}/{app_name}/settings.toml`。
pub trait SettingsFile: Serialize + DeserializeOwned + Default {
    /// 应用名称，决定配置目录名
    fn app_name() -> &'static str;

    /// 配置目录：`{config_dir}/{app_name}/`
    fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join(Self::app_name()))
    }

    /// 配置文件路径：`{config_dir}/{app_name}/settings.toml`
    fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("settings.toml"))
    }

    /// 从磁盘加载，文件不存在或格式错误则返回 `Default`
    fn load() -> Self {
        let Some(path) = Self::config_path() else {
            log::warn!("无法确定配置目录，使用默认配置");
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(settings) => {
                    log::info!("已加载配置：{}", path.display());
                    settings
                }
                Err(e) => {
                    log::warn!("配置解析失败 ({}): {e}，使用默认配置", path.display());
                    Self::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                log::info!("配置文件不存在，使用默认配置");
                Self::default()
            }
            Err(e) => {
                log::warn!("读取配置失败 ({}): {e}，使用默认配置", path.display());
                Self::default()
            }
        }
    }

    /// 保存到磁盘，自动创建目录
    fn save(&self) -> io::Result<()> {
        let Some(dir) = Self::config_dir() else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "无法确定配置目录"));
        };
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("settings.toml");
        let content =
            toml::to_string_pretty(self).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        std::fs::write(&path, content)?;
        log::info!("配置已保存：{}", path.display());
        Ok(())
    }
}
