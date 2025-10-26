use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::manifest::PluginManifest;
use crate::plugin::{Plugin, PluginData};

pub struct PluginManager {
    plugin_root: PathBuf,
    pub plugins: HashMap<String, Plugin>,
    pub updated: bool,
}

impl PluginManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            plugin_root: root,
            plugins: HashMap::new(),
            updated: false,
        }
    }

    pub async fn add(&mut self, path: &Path) -> Result<()> {
        let plugin = Plugin::load(path.to_path_buf())?;
        let name = plugin.manifest.name.clone();

        self.plugins.insert(name, plugin);
        Ok(())
    }

    pub async fn start_all(&mut self) {
        let mut names: Vec<String> = self.plugins.keys().cloned().collect();
        names.sort();

        for name in names {
            if let Err(err) = self.start_plugin(&name).await {
                log::error!("Failed to start plugin {}: {err}", name);
            }
        }
    }

    pub async fn start_plugin(&mut self, name: &str) -> Result<()> {
        let mut should_remove = false;

        let result = match self.plugins.get_mut(name) {
            Some(plugin) => {
                if plugin.state.disabled {
                    log::info!("Plugin {} is disabled, skip starting", name);
                    return Ok(());
                }

                if plugin.state.loaded {
                    return Ok(());
                }

                match plugin.run().await {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        should_remove = true;
                        plugin.stop();
                        Err(anyhow::anyhow!("plugin '{}' on_load failed. detail: {}", name, err))
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
        let dir = match self.plugins.get(name) {
            Some(p) => p.path.clone(),
            None => {
                log::error!("Plugin {} not found", name);
                return false;
            }
        };
        match self.plugins.remove(name) {
            Some(_) => match fs::remove_dir_all(dir) {
                Ok(_) => true,
                Err(e) => {
                    log::error!("Failed to remove plugin: {:?} error: {:?}", name, e);
                    false
                }
            },
            None => false,
        }
    }

    pub async fn load_from_dir(&mut self) -> Result<()> {
        fs::create_dir_all(&self.plugin_root)?;

        for entry in fs::read_dir(&self.plugin_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Err(e) = self.add(&path).await {
                    log::error!("Failed to load plugin: {:?} error: {:?}", path, e);
                }
            }
        }
        self.start_all().await;
        Ok(())
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
