use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use frontbridge::invoke_frontend;
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

use crate::bindings::astrobox::psys_host;
use crate::manifest::PluginManifest;
use crate::plugin::{CardRegistration, Plugin, PluginData, purge_precompiled_component};
use crate::{PLUGIN_LIST_CHANGED_EVENT, PLUGINSYSTEM_PROGRESS_EVENT, PluginSystemProgressPayload};

pub struct PluginManager {
    plugin_root: PathBuf,
    app_handle: AppHandle,
    pub plugins: HashMap<String, Plugin>,
    pub updated: bool,
    change_generation: AtomicU64,
}

#[derive(Debug, Serialize)]
struct PluginChangedPayload<'a> {
    name: &'a str,
    action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    loaded: bool,
    disabled: bool,
    generation: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisteredProviderDescriptor {
    pub name: String,
    #[serde(rename = "pluginName")]
    pub plugin_name: String,
    #[serde(rename = "providerType")]
    pub provider_type: String,
}

const FRONT_STORAGE_GET_JSON_METHOD: &str = "host/storage/local/get_json";
const FRONT_STORAGE_SET_JSON_METHOD: &str = "host/storage/local/set_json";
const PLUGIN_DISABLED_STORAGE_KEY: &str = "astrobox.plugin.disabled_map";
const MAX_PLUGIN_PACKAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PLUGIN_PACKAGE_FILES: usize = 2048;
const MAX_PLUGIN_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;

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
    fn provider_type_label(provider_type: &psys_host::register::ProviderType) -> &'static str {
        match provider_type {
            psys_host::register::ProviderType::Url => "url",
            psys_host::register::ProviderType::Custom => "custom",
        }
    }

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

    fn emit_plugin_changed(
        &self,
        name: &str,
        action: &str,
        version: Option<&str>,
        loaded: bool,
        disabled: bool,
    ) {
        let generation = self.change_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let payload = PluginChangedPayload {
            name,
            action,
            version,
            loaded,
            disabled,
            generation,
        };
        if let Err(err) = self.app_handle.emit(PLUGIN_LIST_CHANGED_EVENT, &payload) {
            log::error!("Failed to emit plugin changed event: {err}");
        }
    }

    fn operation_token() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        format!("{}-{}", std::process::id(), timestamp)
    }

    fn staging_dir(&self, name: &str) -> PathBuf {
        let root = self
            .plugin_root
            .parent()
            .unwrap_or(self.plugin_root.as_path())
            .join(".astrobox-plugin-staging");
        root.join(format!("{}-{}", name, Self::operation_token()))
    }

    fn backup_dir(&self, name: &str) -> PathBuf {
        let root = self
            .plugin_root
            .parent()
            .unwrap_or(self.plugin_root.as_path())
            .join(".astrobox-plugin-backup");
        root.join(format!("{}-{}", name, Self::operation_token()))
    }

    pub fn new(root: PathBuf, app_handle: AppHandle) -> Self {
        Self {
            plugin_root: root,
            app_handle,
            plugins: HashMap::new(),
            updated: false,
            change_generation: AtomicU64::new(0),
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
        log::info!("[plugin:{}] Loaded", name);
        Ok(())
    }

    pub async fn start_all(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self.plugins.keys().cloned().collect();
        names.sort();
        let mut errors = Vec::new();

        for name in names {
            if let Err(err) = self.start_plugin(&name).await {
                log::error!("[plugin:{}] Failed to start: {err}", name);
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
                    log::info!("[plugin:{}] Disabled, skip starting", name);
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
                            "Failed to load plugin from {}: {}",
                            plugin.path.to_string_lossy(),
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

    fn create_staging_dir(&self, name: &str) -> Result<PathBuf> {
        let staging_dir = self.staging_dir(name);
        if let Some(parent) = staging_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }
        fs::create_dir_all(&staging_dir)?;
        Ok(staging_dir)
    }

    fn recover_auxiliary_dirs(&self) -> Result<()> {
        let staging_root = self
            .plugin_root
            .parent()
            .unwrap_or(self.plugin_root.as_path())
            .join(".astrobox-plugin-staging");
        if staging_root.exists() {
            fs::remove_dir_all(&staging_root)?;
        }

        let backup_root = self
            .plugin_root
            .parent()
            .unwrap_or(self.plugin_root.as_path())
            .join(".astrobox-plugin-backup");
        if !backup_root.is_dir() {
            return Ok(());
        }

        fs::create_dir_all(&self.plugin_root)?;
        for entry in fs::read_dir(&backup_root)? {
            let backup = entry?.path();
            if !backup.is_dir() {
                let _ = fs::remove_file(backup);
                continue;
            }

            let Some(manifest) = PluginManifest::load_from_dir(&backup).ok() else {
                let _ = fs::remove_dir_all(backup);
                continue;
            };
            let destination = self.plugin_root.join(&manifest.name);
            if destination.exists() {
                let _ = fs::remove_dir_all(backup);
            } else if let Err(err) = fs::rename(&backup, &destination) {
                log::warn!(
                    "[pluginsystem] failed to restore interrupted plugin update {} -> {}: {err}",
                    backup.display(),
                    destination.display()
                );
            }
        }

        let _ = fs::remove_dir(&backup_root);
        Ok(())
    }

    async fn rollback_plugin_swap(
        &mut self,
        name: &str,
        destination: &Path,
        staging_dir: &Path,
        backup_dir: &Path,
        had_destination: bool,
        previous_state: Option<(bool, bool)>,
    ) -> Result<()> {
        if destination.exists() {
            if let Ok(manifest) = PluginManifest::load_from_dir(destination) {
                let _ = purge_precompiled_component(destination, &manifest);
            }
            fs::remove_dir_all(destination).with_context(|| {
                format!(
                    "failed to remove failed plugin replacement {}",
                    destination.display()
                )
            })?;
        }

        if had_destination && backup_dir.exists() {
            fs::rename(backup_dir, destination).with_context(|| {
                format!(
                    "failed to restore plugin backup {} to {}",
                    backup_dir.display(),
                    destination.display()
                )
            })?;
        }

        if staging_dir.exists() {
            let _ = fs::remove_dir_all(staging_dir);
        }

        let Some(previous_state) = previous_state else {
            return Ok(());
        };
        if !destination.is_dir() {
            return Err(anyhow!(
                "plugin '{}' rollback has no restorable destination",
                name
            ));
        }

        self.restore_plugin_after_swap_failure(name, destination, Some(previous_state))
            .await
            .with_context(|| format!("failed to restore plugin '{}' during rollback", name))
    }

    async fn load_plugin_from_path(&self, path: PathBuf) -> Result<Plugin> {
        let app_handle = self.app_handle.clone();
        tokio::task::spawn_blocking(move || Plugin::load(path, app_handle))
            .await
            .context("plugin preparation task failed")?
    }

    async fn restore_plugin_after_swap_failure(
        &mut self,
        name: &str,
        destination: &Path,
        previous_state: Option<(bool, bool)>,
    ) -> Result<()> {
        let Some((was_loaded, was_disabled)) = previous_state else {
            return Ok(());
        };

        let mut restored = self
            .load_plugin_from_path(destination.to_path_buf())
            .await
            .with_context(|| format!("failed to reload plugin '{}' after swap failure", name))?;
        if was_loaded && !was_disabled {
            restored.run().await.with_context(|| {
                format!("failed to restart plugin '{}' after swap failure", name)
            })?;
        } else {
            restored.state.disabled = was_disabled;
            restored.state.loaded = false;
        }
        self.plugins.insert(name.to_string(), restored);
        Ok(())
    }

    // The install command must not return until the swap and first start have
    // completed. Otherwise the frontend reports success while Windows may
    // still be failing to rename or load the staged directory in the
    // background.
    async fn activate_staged_plugin(&mut self, name: String, staging_dir: PathBuf) -> Result<()> {
        self.updated = true;
        self.emit_progress(&name, "activate", None);

        let destination = self.plugin_root.join(&name);
        fs::create_dir_all(&self.plugin_root)?;
        let previous_state = self
            .plugins
            .get(&name)
            .map(|plugin| (plugin.state.loaded, plugin.state.disabled));
        let had_destination = destination.exists();
        let backup_dir = self.backup_dir(&name);
        if had_destination {
            if let Some(parent) = backup_dir.parent() {
                fs::create_dir_all(parent)?;
            }
            if backup_dir.exists() {
                fs::remove_dir_all(&backup_dir)?;
            }
        }

        // Stop and release the old runtime before renaming its directory. The
        // replacement is intentionally loaded only after the staged directory
        // reaches its final location.
        let mut old_plugin = self.plugins.remove(&name);
        if let Some(plugin) = old_plugin.as_mut() {
            plugin.stop().await;
        }
        drop(old_plugin);

        if had_destination {
            if let Err(err) = fs::rename(&destination, &backup_dir) {
                let _ = fs::remove_dir_all(&staging_dir);
                let restore = self
                    .restore_plugin_after_swap_failure(&name, &destination, previous_state)
                    .await;
                return match restore {
                    Ok(()) => Err(err).with_context(|| {
                        format!(
                            "failed to backup plugin directory {} to {}",
                            destination.display(),
                            backup_dir.display()
                        )
                    }),
                    Err(restore_err) => Err(anyhow!(
                        "failed to backup plugin directory {} to {}: {}; restore failed: {}",
                        destination.display(),
                        backup_dir.display(),
                        err,
                        restore_err
                    )),
                };
            }
        }

        if let Err(err) = fs::rename(&staging_dir, &destination) {
            let rollback = self
                .rollback_plugin_swap(
                    &name,
                    &destination,
                    &staging_dir,
                    &backup_dir,
                    had_destination,
                    previous_state,
                )
                .await;
            return match rollback {
                Ok(()) => {
                    self.emit_current_plugin_state(&name, "rollback");
                    Err(anyhow!("failed to activate plugin '{}': {err}", name))
                }
                Err(rollback_err) => Err(anyhow!(
                    "failed to activate plugin '{}': {}; rollback failed: {}",
                    name,
                    err,
                    rollback_err
                )),
            };
        }

        // Load the component only after the staged directory has reached its
        // final location. This avoids Windows rename failures caused by the
        // component mapping still referencing a file under the staging path.
        let mut new_plugin = match self.load_plugin_from_path(destination.clone()).await {
            Ok(plugin) => plugin,
            Err(err) => {
                let rollback = self
                    .rollback_plugin_swap(
                        &name,
                        &destination,
                        &staging_dir,
                        &backup_dir,
                        had_destination,
                        previous_state,
                    )
                    .await;
                return match rollback {
                    Ok(()) => {
                        self.emit_current_plugin_state(&name, "rollback");
                        Err(anyhow!("failed to load plugin '{}': {}", name, err))
                    }
                    Err(rollback_err) => Err(anyhow!(
                        "failed to load plugin '{}': {}; rollback failed: {}",
                        name,
                        err,
                        rollback_err
                    )),
                };
            }
        };

        let should_start = previous_state
            .map(|(was_loaded, was_disabled)| was_loaded && !was_disabled)
            .unwrap_or(true);
        let should_remain_disabled = previous_state
            .map(|(_, was_disabled)| was_disabled)
            .unwrap_or(false);

        if should_start {
            if let Err(err) = new_plugin.run().await {
                new_plugin.stop().await;
                drop(new_plugin);
                let rollback = self
                    .rollback_plugin_swap(
                        &name,
                        &destination,
                        &staging_dir,
                        &backup_dir,
                        had_destination,
                        previous_state,
                    )
                    .await;
                return match rollback {
                    Ok(()) => {
                        self.emit_current_plugin_state(&name, "rollback");
                        Err(anyhow!("failed to start plugin '{}': {}", name, err))
                    }
                    Err(rollback_err) => Err(anyhow!(
                        "failed to start plugin '{}': {}; rollback failed: {}",
                        name,
                        err,
                        rollback_err
                    )),
                };
            }
        } else {
            new_plugin.state.disabled = should_remain_disabled;
            new_plugin.state.loaded = false;
        }

        let action = if previous_state.is_some() || had_destination {
            "updated"
        } else {
            "installed"
        };
        self.plugins.insert(name.clone(), new_plugin);

        if backup_dir.exists() {
            if let Err(err) = fs::remove_dir_all(&backup_dir) {
                log::warn!(
                    "[plugin:{}] failed to remove old plugin backup {}: {err}",
                    name,
                    backup_dir.display()
                );
            }
        }
        if action == "installed" {
            self.set_plugin_disabled_persisted(&name, false).await;
        }
        self.emit_current_plugin_state(&name, action);
        Ok(())
    }

    fn emit_current_plugin_state(&self, name: &str, action: &str) {
        if let Some(plugin) = self.plugins.get(name) {
            self.emit_plugin_changed(
                name,
                action,
                Some(plugin.manifest.version.as_str()),
                plugin.state.loaded,
                plugin.state.disabled,
            );
        } else {
            self.emit_plugin_changed(name, action, None, false, true);
        }
    }

    pub async fn add_from_dir(&mut self, _name: &str, path: &Path) -> Result<()> {
        if !path.is_dir() {
            return Err(anyhow!("source path is not a directory"));
        }
        let manifest = PluginManifest::load_from_dir(path)?;
        let staging_dir = self.create_staging_dir(&manifest.name)?;
        if let Err(err) = copy_dir_recursive(path, &staging_dir) {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(err);
        }
        self.activate_staged_plugin(manifest.name, staging_dir)
            .await
    }

    pub async fn add_from_abp(&mut self, _name: &str, path: &Path) -> Result<()> {
        let package_raw = tokio::fs::read(path).await?;
        if package_raw.len() > MAX_PLUGIN_PACKAGE_BYTES {
            return Err(anyhow!(
                "plugin package is too large: {} bytes (limit {} bytes)",
                package_raw.len(),
                MAX_PLUGIN_PACKAGE_BYTES
            ));
        }
        let manifest = resolve_manifest_from_abp(&package_raw)?;
        let staging_dir = self.create_staging_dir(&manifest.name)?;
        if let Err(err) = extract_abp_to_dir(&package_raw, &staging_dir) {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(err);
        }
        self.activate_staged_plugin(manifest.name, staging_dir)
            .await
    }

    pub async fn enable(&mut self, name: &String) -> bool {
        log::info!("[plugin:{}] Enable requested", name);
        self.updated = true;
        let mut changed = false;
        if let Some(plugin) = self.plugins.get_mut(name) {
            if plugin.state.loaded && !plugin.state.disabled {
                log::info!("[plugin:{}] Already enabled", name);
                changed = true;
            } else {
                plugin.state.disabled = false;

                match plugin.run().await {
                    Ok(()) => {
                        log::info!("Enable successful");
                        changed = true;
                    }
                    Err(err) => {
                        log::error!("[plugin:{}] Failed to start: {err}", name);
                        plugin.stop().await;
                    }
                }
            }
        }

        if changed {
            self.set_plugin_disabled_persisted(name, false).await;
            if let Some(plugin) = self.plugins.get(name) {
                self.emit_plugin_changed(
                    name,
                    "enabled",
                    Some(plugin.manifest.version.as_str()),
                    plugin.state.loaded,
                    plugin.state.disabled,
                );
            }
            return true;
        }
        false
    }

    async fn take_plugin_for_cleanup(
        &mut self,
        plugin_name: &str,
    ) -> Option<(PathBuf, PluginManifest)> {
        match self.plugins.remove(plugin_name) {
            Some(mut plugin) => {
                plugin.stop().await;
                Some((plugin.path, plugin.manifest))
            }
            None => None,
        }
    }

    pub async fn dispatch_interconnect_message(
        &mut self,
        addr: &str,
        pkg_name: &str,
        payload: String,
    ) {
        let mut active_plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
            .collect::<Vec<_>>();
        active_plugins.sort_by(|left, right| left.0.cmp(&right.0));

        let mut matched = Vec::new();
        for (name, runtime) in active_plugins {
            if runtime.matches_interconnect(addr, pkg_name).await {
                matched.push((name, runtime));
            }
        }

        if matched.is_empty() {
            return;
        }

        log::debug!(
            "[pluginsystem] interconnect dispatch addr={} pkg={} -> {} receiver(s)",
            addr,
            pkg_name,
            matched.len()
        );

        let mut handles = Vec::with_capacity(matched.len());
        for (name, runtime) in matched {
            let payload = payload.clone();
            handles.push(tokio::spawn(async move {
                if let Err(err) = runtime.dispatch_interconnect_message(payload).await {
                    log::error!(
                        "[plugin:{}] Failed to deliver interconnect message: {err}",
                        name
                    );
                }
            }));
        }

        for handle in join_all(handles).await {
            if let Err(err) = handle {
                log::error!("[pluginsystem] interconnect dispatch task panicked: {err}");
            }
        }
    }

    pub async fn dispatch_transport_packet(
        &mut self,
        addr: &str,
        channel_id: u32,
        protobuf_type_id: Option<u32>,
        protobuf_packet_id: Option<u32>,
        payload: Vec<u8>,
    ) {
        crate::transport_runtime::fulfill_request_waiters(
            addr,
            channel_id,
            protobuf_type_id,
            protobuf_packet_id,
            &payload,
        );

        let mut active_plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
            .collect::<Vec<_>>();
        active_plugins.sort_by(|left, right| left.0.cmp(&right.0));

        let mut matched = Vec::new();
        for (name, runtime) in active_plugins {
            if runtime
                .matches_transport(addr, channel_id, protobuf_type_id)
                .await
            {
                matched.push((name, runtime));
            }
        }

        if matched.is_empty() {
            return;
        }

        let payload_base64 = BASE64_STANDARD.encode(&payload);
        log::debug!(
            "[pluginsystem] transport dispatch addr={} channel={} -> {} receiver(s)",
            addr,
            channel_id,
            matched.len()
        );

        let mut handles = Vec::with_capacity(matched.len());
        for (name, runtime) in matched {
            let payload = payload_base64.clone();
            handles.push(tokio::spawn(async move {
                if let Err(err) = runtime.dispatch_transport_packet(payload).await {
                    log::error!(
                        "[plugin:{}] Failed to deliver transport packet: {err}",
                        name
                    );
                }
            }));
        }

        for handle in join_all(handles).await {
            if let Err(err) = handle {
                log::error!("[pluginsystem] transport dispatch task panicked: {err}");
            }
        }
    }

    pub async fn disable(&mut self, name: &String) -> bool {
        log::info!("[plugin:{}] Disable requested", name);
        self.updated = true;
        let changed = if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.stop().await;
            log::info!("Disable successful");
            true
        } else {
            false
        };

        if changed {
            self.set_plugin_disabled_persisted(name, true).await;
            if let Some(plugin) = self.plugins.get(name) {
                self.emit_plugin_changed(
                    name,
                    "disabled",
                    Some(plugin.manifest.version.as_str()),
                    plugin.state.loaded,
                    plugin.state.disabled,
                );
            }
            true
        } else {
            false
        }
    }

    pub async fn remove(&mut self, name: &String) -> bool {
        self.updated = true;
        let (plugin_path, plugin_manifest) = match self.take_plugin_for_cleanup(name).await {
            Some(plugin) => plugin,
            None => {
                log::error!("[plugin:{}] Not found", name);
                return false;
            }
        };

        if let Err(err) = purge_precompiled_component(&plugin_path, &plugin_manifest) {
            log::warn!(
                "[plugin:{}] Failed to purge precompiled artifacts: {err}",
                name
            );
        }

        match fs::remove_dir_all(&plugin_path) {
            Ok(_) => {
                self.set_plugin_disabled_persisted(name, false).await;
                self.emit_plugin_changed(
                    name,
                    "removed",
                    Some(plugin_manifest.version.as_str()),
                    false,
                    true,
                );
                true
            }
            Err(e) => {
                log::error!("[plugin:{}] Failed to remove: {e:?}", name);
                false
            }
        }
    }

    pub async fn clear_disabled_flag_for_folder(&mut self, folder_name: &str) {
        let name = self
            .plugins
            .values()
            .find(|plugin| plugin.path.file_name().and_then(|n| n.to_str()) == Some(folder_name))
            .map(|plugin| plugin.manifest.name.clone())
            .unwrap_or_else(|| folder_name.to_string());
        self.set_plugin_disabled_persisted(&name, false).await;
    }

    pub async fn load_from_dir(&mut self) -> Result<Vec<String>> {
        fs::create_dir_all(&self.plugin_root)?;
        self.recover_auxiliary_dirs()?;
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

    pub async fn list_providers(&self) -> Vec<RegisteredProviderDescriptor> {
        let mut providers = Vec::new();
        let mut seen = HashSet::new();
        let mut active_plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
            .collect::<Vec<_>>();
        active_plugins.sort_by(|left, right| left.0.cmp(&right.0));

        for (plugin_name, runtime) in active_plugins {
            for registration in runtime.list_providers().await {
                if !seen.insert(registration.name.clone()) {
                    log::warn!(
                        "[pluginsystem] duplicate provider registration ignored: provider={}, plugin={}",
                        registration.name,
                        plugin_name
                    );
                    continue;
                }

                providers.push(RegisteredProviderDescriptor {
                    name: registration.name,
                    plugin_name: plugin_name.clone(),
                    provider_type: Self::provider_type_label(&registration.provider_type)
                        .to_string(),
                });
            }
        }

        providers
    }

    pub async fn call_provider_action(&self, provider_name: &str, payload: String) -> Result<()> {
        let mut active_plugins = self
            .plugins
            .iter()
            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
            .collect::<Vec<_>>();
        active_plugins.sort_by(|left, right| left.0.cmp(&right.0));

        let mut matches = Vec::new();
        for (plugin_name, runtime) in active_plugins {
            if runtime
                .list_providers()
                .await
                .into_iter()
                .any(|registration| registration.name == provider_name)
            {
                matches.push((plugin_name, runtime));
            }
        }

        if matches.len() > 1 {
            log::warn!(
                "[pluginsystem] provider '{}' is registered by multiple plugins; using '{}'",
                provider_name,
                matches[0].0
            );
        }

        let Some((plugin_name, runtime)) = matches.into_iter().next() else {
            return Err(anyhow!("Plugin provider '{}' not found", provider_name));
        };

        runtime
            .dispatch_provider_action(payload)
            .await
            .with_context(|| {
                format!(
                    "Plugin provider action failed. provider={}, plugin={}",
                    provider_name, plugin_name
                )
            })
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

fn extract_abp_to_dir(package_raw: &[u8], destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let reader = Cursor::new(package_raw);
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > MAX_PLUGIN_PACKAGE_FILES {
        return Err(anyhow!(
            "plugin package contains too many files: {} (limit {})",
            archive.len(),
            MAX_PLUGIN_PACKAGE_FILES
        ));
    }
    let mut seen_paths = HashSet::new();
    let mut unpacked_bytes = 0u64;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let relative_path = file.mangled_name();
        let outpath = destination.join(&relative_path);
        if !outpath.starts_with(destination) {
            return Err(anyhow!(
                "plugin package contains an unsafe path: {}",
                file.name()
            ));
        }
        if !seen_paths.insert(outpath.clone()) {
            return Err(anyhow!(
                "plugin package contains a duplicate path: {}",
                file.name()
            ));
        }
        unpacked_bytes = unpacked_bytes.saturating_add(file.size());
        if unpacked_bytes > MAX_PLUGIN_UNPACKED_BYTES {
            return Err(anyhow!(
                "plugin package expands beyond the limit of {} bytes",
                MAX_PLUGIN_UNPACKED_BYTES
            ));
        }

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
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

    Ok(())
}

fn resolve_manifest_from_abp(package_raw: &[u8]) -> Result<PluginManifest> {
    let reader = Cursor::new(package_raw);
    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name().ends_with('/') {
            continue;
        }

        let file_name = file.mangled_name();
        let is_manifest = file_name
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("manifest.json"))
            .unwrap_or(false);
        if !is_manifest {
            continue;
        }

        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let manifest: PluginManifest = serde_json::from_str(&data)
            .context("Failed to resolve plugin manifest from plugin package")?;
        manifest.validate(std::path::Path::new("manifest.json (plugin package)"))?;
        return Ok(manifest);
    }

    Err(anyhow!("manifest.json not found in plugin package"))
}
