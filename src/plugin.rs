use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::manifest::PluginManifest;

#[derive(Default)]
pub struct PluginState {
    pub disabled: bool,
}

#[derive(Default)]
pub struct PluginData {
    pub metadata: HashMap<String, String>,
}

pub struct PluginRuntime {
    _root: PathBuf,
}

impl PluginRuntime {
    pub fn initialise(path: &Path, _manifest: &PluginManifest) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("插件目录不存在: {}", path.display()));
        }

        Ok(Self {
            _root: path.to_path_buf(),
        })
    }
}

pub struct Plugin {
    pub path: PathBuf,
    pub manifest: PluginManifest,
    pub runtime: Option<PluginRuntime>,
    pub data: PluginData,
    pub state: PluginState,
}

impl Plugin {
    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.is_dir() {
            return Err(anyhow!("插件路径无效: {}", path.display()));
        }

        let manifest = PluginManifest::load_from_dir(&path)?;

        Ok(Self {
            path,
            manifest,
            runtime: None,
            data: PluginData::default(),
            state: PluginState::default(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let runtime = PluginRuntime::initialise(&self.path, &self.manifest)?;
        self.runtime = Some(runtime);
        self.state.disabled = false;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.runtime = None;
        self.state.disabled = true;
    }

}
