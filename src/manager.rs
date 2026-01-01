use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

use crate::manifest::PluginManifest;
use crate::{PLUGINSYSTEM_PROGRESS_EVENT, PluginSystemProgressPayload};
use crate::plugin::{Plugin, PluginData, purge_precompiled_component};

pub struct PluginManager {
    plugin_root: PathBuf,
    app_handle: AppHandle,
    pub plugins: HashMap<String, Plugin>,
    pub updated: bool,
}

impl PluginManager {
    fn emit_progress(&self, plugin: &str, stage: &str, detail: Option<String>) {
        let payload = PluginSystemProgressPayload {
            plugin: plugin.to_string(),
            stage: stage.to_string(),
            detail,
        };
        if let Err(err) = self
            .app_handle
            .emit(PLUGINSYSTEM_PROGRESS_EVENT, &payload)
        {
            log::error!("Failed to emit plugin progress event: {err}");
        }
    }

    pub fn new(root: PathBuf, app_handle: AppHandle) -> Self {
        Self {
            plugin_root: root,
            app_handle,
            plugins: HashMap::new(),
            updated: false,
        }
    }

    pub async fn add(&mut self, path: &Path) -> Result<()> {
        let dir_label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-plugin");
        self.emit_progress(dir_label, "load", None);
        log::info!(
            "Loading plugin from path {}",
            path.to_string_lossy().to_string()
        );
        let plugin = Plugin::load(path.to_path_buf(), self.app_handle.clone())?;
        let name = plugin.manifest.name.clone();

        self.plugins.insert(name.clone(), plugin);
        self.emit_progress(&name, "loaded", None);
        log::info!("Plugin {} loaded!", name);
        Ok(())
    }

    pub async fn start_all(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self.plugins.keys().cloned().collect();
        names.sort();
        let mut errors = Vec::new();

        for name in names {
            if let Err(err) = self.start_plugin(&name).await {
                log::error!("Failed to start plugin {}: {err}", name);
                errors.push(err.to_string());
            }
        }

        errors
    }

    pub async fn start_plugin(&mut self, name: &str) -> Result<()> {
        let mut should_remove = false;
        let app_handle = self.app_handle.clone();
        let emit_progress =
            |plugin: &str, stage: &str, detail: Option<String>| {
                let payload = PluginSystemProgressPayload {
                    plugin: plugin.to_string(),
                    stage: stage.to_string(),
                    detail,
                };
                if let Err(err) =
                    app_handle.emit(PLUGINSYSTEM_PROGRESS_EVENT, &payload)
                {
                    log::error!("Failed to emit plugin progress event: {err}");
                }
            };

        let result = match self.plugins.get_mut(name) {
            Some(plugin) => {
                if plugin.state.disabled {
                    log::info!("Plugin {} is disabled, skip starting", name);
                    emit_progress(name, "disabled", None);
                    return Ok(());
                }

                if plugin.state.loaded {
                    emit_progress(name, "ready", None);
                    return Ok(());
                }

                emit_progress(name, "start", None);
                match plugin.run().await {
                    Ok(()) => {
                        emit_progress(name, "ready", None);
                        Ok(())
                    }
                    Err(err) => {
                        should_remove = true;
                        plugin.stop();
                        emit_progress(name, "error", Some(err.to_string()));
                        Err(anyhow::anyhow!(
                            "plugin '{}' on_load failed. detail: {}",
                            name,
                            err
                        ))
                    }
                }
            }
            None => Err(corelib::anyhow_site!("Plugin '{}' not found", name)),
        };

        if should_remove {
            self.plugins.remove(name);
        }

        result
    }

    pub async fn add_from_abp(&mut self, name: &String, path: &String) -> Result<()> {
        self.updated = true;
        let package_raw = tokio::fs::read(path).await?;
        let reader = Cursor::new(package_raw);
        let mut archive = ZipArchive::new(reader)?;

        let dest_dir = self.plugin_root.join(name);

        if !dest_dir.exists() {
            fs::create_dir_all(&dest_dir)?;
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = dest_dir.join(file.mangled_name());

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                }
            }
        }

        self.add(&dest_dir).await?;
        self.start_plugin(name).await?;

        Ok(())
    }

    pub async fn enable(&mut self, name: &String) -> bool {
        log::info!("Enable plugin {}", name);
        self.updated = true;
        if let Some(plugin) = self.plugins.get_mut(name) {
            if plugin.state.loaded && !plugin.state.disabled {
                log::info!("Plugin {} already enabled", name);
                return true;
            }

            plugin.state.disabled = false;

            match plugin.run().await {
                Ok(()) => {
                    log::info!("Enable successful");
                    return true;
                }
                Err(err) => {
                    log::error!("Failed to start plugin {}: {err}", name);
                    plugin.stop();
                }
            }
        }

        false
    }

    pub async fn dispatch_interconnect_message(
        &mut self,
        addr: &str,
        pkg_name: &str,
        payload: String,
    ) {
        let active_plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
            .collect::<Vec<_>>();

        for (name, runtime) in active_plugins {
            if !runtime.matches_interconnect(addr, pkg_name).await {
                continue;
            }

            if let Err(err) = runtime
                .dispatch_interconnect_message(payload.clone())
                .await
            {
                log::error!(
                    "Failed to deliver interconnect message to {}: {err}",
                    name
                );
            }
        }
    }

    pub fn disable(&mut self, name: &String) -> bool {
        log::info!("Disable plugin {}", name);
        self.updated = true;
        match self.plugins.get_mut(name) {
            Some(plug) => {
                plug.stop();
                log::info!("Disable successful");
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, name: &String) -> bool {
        self.updated = true;
        let plugin = match self.plugins.remove(name) {
            Some(plugin) => plugin,
            None => {
                log::error!("Plugin {} not found", name);
                return false;
            }
        };

        if let Err(err) = purge_precompiled_component(&plugin.path, &plugin.manifest) {
            log::warn!(
                "Failed to purge precompiled artifacts for plugin {}: {err}",
                name
            );
        }

        match fs::remove_dir_all(&plugin.path) {
            Ok(_) => true,
            Err(e) => {
                log::error!("Failed to remove plugin: {:?} error: {:?}", name, e);
                false
            }
        }
    }

    pub async fn load_from_dir(&mut self) -> Result<Vec<String>> {
        fs::create_dir_all(&self.plugin_root)?;
        let mut errors = Vec::new();

        for entry in fs::read_dir(&self.plugin_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = self.add(&path).await {
                    let detail = format!(
                        "Failed to load plugin from {}: {e}",
                        path.to_string_lossy()
                    );
                    log::error!("{detail}");
                    let label = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown-plugin");
                    self.emit_progress(label, "error", Some(detail.clone()));
                    errors.push(detail);
                }
            }
        }
        errors.extend(self.start_all().await);
        Ok(errors)
    }

    pub fn set_plugin_data<F>(&mut self, name: &str, f: F) -> Result<()>
    where
        F: FnOnce(&mut PluginData),
    {
        if let Some(plugin) = self.plugins.get_mut(name) {
            f(&mut plugin.data);
            Ok(())
        } else {
            Err(corelib::anyhow_site!("Plugin '{}' not found", name))
        }
    }

    pub fn get(&mut self, name: &str) -> Option<&mut Plugin> {
        self.plugins.get_mut(name)
    }

    pub fn list(&self) -> Vec<PluginManifest> {
        let plugs = self
            .plugins
            .values()
            .map(|pl| pl.manifest.clone())
            .collect();

        match serde_json::to_string(&plugs) {
            Ok(s) => log::info!("Get plugin list: {}", s),
            Err(e) => log::error!("Serialize plugin list failed: {}", e),
        }
        plugs
    }

    pub fn is_updated(&self) -> bool {
        self.updated
    }
}
