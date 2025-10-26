use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,             // 插件名称
    pub icon: String,             // 插件图标（路径）
    pub version: String,          // 插件版本
    pub description: String,      // 插件简介
    pub author: String,           // 插件作者
    pub website: String,          // 插件网站（例如github仓库地址）
    pub entry: String,            // 插件入口wasm文件
    pub wasi_version: u32,        // WASI 接口版本
    pub api_level: u32,           // 插件api等级
    pub permissions: Vec<String>, // 插件权限列表
    #[serde(default)]
    pub additional_files: Vec<String>, // 插件附加文件列表
}

impl PluginManifest {
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join("manifest.json");
        let data = fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "Failed to read plugin manifest: {}",
                manifest_path.display()
            )
        })?;
        let manifest: PluginManifest = serde_json::from_str(&data).with_context(|| {
            format!(
                "Failed to resolve plugin manifest: {}",
                manifest_path.display()
            )
        })?;
        if manifest.name.trim().is_empty() {
            return Err(corelib::anyhow_site!(
                "name is empty in manifest: {}",
                manifest_path.display()
            ));
        }
        Ok(manifest)
    }

    pub fn entry_wasm_path(&self, base_dir: &Path) -> PathBuf {
        let entry = self.entry.clone();
        base_dir.join(entry)
    }
}
