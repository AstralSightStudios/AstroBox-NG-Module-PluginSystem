use anyhow::{Result, anyhow};
use frontbridge::invoke_frontend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

use crate::manifest::PluginManifest;
use crate::plugin::{CardRegistration, Plugin, PluginData, purge_precompiled_component};
use crate::{PLUGINSYSTEM_PROGRESS_EVENT, PluginSystemProgressPayload};

pub struct PluginManager {
    plugin_root: PathBuf,
    app_handle: AppHandle,
    pub plugins: HashMap<String, Plugin>,
    pub updated: bool,
}

const FRONT_STORAGE_GET_JSON_METHOD: &str = "host/storage/local/get_json";
const FRONT_STORAGE_SET_JSON_METHOD: &str = "host/storage/local/set_json";
const PLUGIN_DISABLED_STORAGE_KEY: &str = "astrobox.plugin.disabled_map";

#[derive(Serialize)]
struct LocalStorageKeyPayload {
    key: String,
}

#[derive(Serialize)]
struct LocalStorageSetPayload<T> {
    key: String,
    value: T,
}

#[derive(Deserialize)]
struct LocalStorageAck {
    success: bool,
}

impl PluginManager {
    fn emit_progress(&self, plugin: &str, stage: &str, detail: Option<String>) {
        let payload = PluginSystemProgressPayload {
            plugin: plugin.to_string(),
            stage: stage.to_string(),
            detail,
        };
        if let Err(err) = self.app_handle.emit(PLUGINSYSTEM_PROGRESS_EVENT, &payload) {
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

    async fn load_disabled_map(&self) -> std::collections::HashMap<String, bool> {
        let payload = LocalStorageKeyPayload {
            key: PLUGIN_DISABLED_STORAGE_KEY.to_string(),
        };
        match invoke_frontend::<Option<std::collections::HashMap<String, bool>>, _>(
            &self.app_handle,
            FRONT_STORAGE_GET_JSON_METHOD,
            payload,
        )
        .await
        {
            Ok(Some(map)) => map,
            Ok(None) => std::collections::HashMap::new(),
            Err(err) => {
                log::warn!("[pluginsystem] failed to load disabled map: {err}");
                std::collections::HashMap::new()
            }
        }
    }

    async fn store_disabled_map(&self, map: &std::collections::HashMap<String, bool>) {
        let payload = LocalStorageSetPayload {
            key: PLUGIN_DISABLED_STORAGE_KEY.to_string(),
            value: map,
        };
        match invoke_frontend::<LocalStorageAck, _>(
            &self.app_handle,
            FRONT_STORAGE_SET_JSON_METHOD,
            payload,
        )
        .await
        {
            Ok(resp) => {
                if !resp.success {
                    log::warn!("[pluginsystem] store disabled map rejected");
                }
            }
            Err(err) => {
                log::warn!("[pluginsystem] failed to store disabled map: {err}");
            }
        }
    }

    async fn set_plugin_disabled_persisted(&self, name: &str, disabled: bool) {
        let mut map = self.load_disabled_map().await;
        if disabled {
            map.insert(name.to_string(), true);
        } else {
            map.remove(name);
        }
        self.store_disabled_map(&map).await;
    }

    pub async fn start_plugin(&mut self, name: &str) -> Result<()> {
        let mut should_remove = false;
        let app_handle = self.app_handle.clone();
        let emit_progress = |plugin: &str, stage: &str, detail: Option<String>| {
            let payload = PluginSystemProgressPayload {
                plugin: plugin.to_string(),
                stage: stage.to_string(),
                detail,
            };
            if let Err(err) = app_handle.emit(PLUGINSYSTEM_PROGRESS_EVENT, &payload) {
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
                        plugin.stop().await;
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

    pub async fn add_from_dir(&mut self, name: &str, path: &Path) -> Result<()> {
        self.updated = true;
        if !path.is_dir() {
            return Err(anyhow!("source path is not a directory"));
        }
        let dest_dir = self.plugin_root.join(name);
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir)?;
        }
        copy_dir_recursive(path, &dest_dir)?;
        Ok(())
    }

    pub async fn add_from_abp(&mut self, name: &str, path: &Path) -> Result<()> {
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

        /*
        self.add(&dest_dir).await?;
        self.set_plugin_disabled_persisted(name, false).await;
        self.start_plugin(name).await?;
        */

        Ok(())
    }

    pub async fn enable(&mut self, name: &String) -> bool {
        log::info!("Enable plugin {}", name);
        self.updated = true;
        if let Some(plugin) = self.plugins.get_mut(name) {
            if plugin.state.loaded && !plugin.state.disabled {
                log::info!("Plugin {} already enabled", name);
                self.set_plugin_disabled_persisted(name, false).await;
                return true;
            }

            plugin.state.disabled = false;

            match plugin.run().await {
                Ok(()) => {
                    log::info!("Enable successful");
                    self.set_plugin_disabled_persisted(name, false).await;
                    return true;
                }
                Err(err) => {
                    log::error!("Failed to start plugin {}: {err}", name);
                    plugin.stop().await;
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

            if let Err(err) = runtime.dispatch_interconnect_message(payload.clone()).await {
                log::error!("Failed to deliver interconnect message to {}: {err}", name);
            }
        }
    }

    pub async fn disable(&mut self, name: &String) -> bool {
        log::info!("Disable plugin {}", name);
        self.updated = true;
        match self.plugins.get_mut(name) {
            Some(plug) => {
                plug.stop().await;
                log::info!("Disable successful");
                self.set_plugin_disabled_persisted(name, true).await;
                true
            }
            None => false,
        }
    }

    pub async fn remove(&mut self, name: &String) -> bool {
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
            Ok(_) => {
                self.set_plugin_disabled_persisted(name, false).await;
                true
            }
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
                    let detail =
                        format!("Failed to load plugin from {}: {e}", path.to_string_lossy());
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
        let disabled_map = self.load_disabled_map().await;
        for (name, plugin) in self.plugins.iter_mut() {
            let disabled = disabled_map.get(name).copied().unwrap_or(false);
            if disabled {
                plugin.state.disabled = true;
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

    pub async fn list_cards(&self) -> Vec<CardRegistration> {
        let mut cards = Vec::new();
        for plugin in self.plugins.values() {
            cards.extend(plugin.runtime.list_cards().await);
        }
        cards
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive(&entry_path, &target_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = target_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::copy(&entry_path, &target_path)?;
        }
    }
    Ok(())
}
