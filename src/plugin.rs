use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tauri::AppHandle;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder, p2};

use crate::api::host::PluginCtx;
use crate::bindings::PsysWorld;
use crate::manifest::PluginManifest;

pub struct PluginState {
    pub disabled: bool,
    pub loaded: bool,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            disabled: false,
            loaded: false,
        }
    }
}

#[derive(Default)]
pub struct PluginData {
    pub metadata: HashMap<String, String>,
}

#[cfg(target_os = "ios")]
fn configure_engine(config: &mut Config) -> Result<()> {
    let pulley_triple = if cfg!(target_pointer_width = "32") {
        if cfg!(target_endian = "big") {
            "pulley32be"
        } else {
            "pulley32"
        }
    } else if cfg!(target_endian = "big") {
        "pulley64be"
    } else {
        "pulley64"
    };

    config.target(pulley_triple).with_context(|| {
        format!("failed to select Wasmtime interpreter target `{pulley_triple}` for iOS")
    })?;

    log::info!(
        "Detected iOS runtime; Wasmtime configured for interpreter mode via target `{pulley_triple}`"
    );

    Ok(())
}

#[cfg(not(target_os = "ios"))]
fn configure_engine(_config: &mut Config) -> Result<()> {
    Ok(())
}

pub struct PluginRuntime {
    name: String,
    engine: Engine,
    component: Component,
    plugin_root: PathBuf,
    app_handle: AppHandle,
}

impl PluginRuntime {
    pub fn initialise(
        path: &Path,
        manifest: &PluginManifest,
        app_handle: AppHandle,
    ) -> Result<Self> {
        if !path.exists() {
            return Err(corelib::anyhow_site!(
                "plugin directory does not exist: {}",
                path.display()
            ));
        }

        let entry_path = manifest.entry_wasm_path(path);
        if !entry_path.is_file() {
            return Err(corelib::anyhow_site!(
                "plugin entry file does not exist: {}",
                entry_path.display()
            ));
        }

        let mut config = Config::default();
        configure_engine(&mut config)?;
        config
            .wasm_component_model(true)
            .wasm_component_model_async(true)
            .async_support(true);

        log::info!(
            "Creating wasmtime engine for plugin {}...",
            manifest.clone().name
        );

        let engine = Engine::new(&config).context("Failed to initialize the Wasmtime engine")?;
        let component = Component::from_file(&engine, &entry_path)
            .with_context(|| format!("Failed to load plugin entry: {}", entry_path.display()))?;

        Ok(Self {
            name: manifest.name.clone(),
            engine,
            component,
            plugin_root: path.to_path_buf(),
            app_handle,
        })
    }

    fn build_wasi_ctx(&self) -> Result<WasiCtx> {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdout().inherit_stderr();

        builder
            .preopened_dir(&self.plugin_root, ".", DirPerms::all(), FilePerms::all())
            .with_context(|| {
                format!(
                    "Failed to pre-open directory for plugin: {}",
                    self.plugin_root.display()
                )
            })?;

        Ok(builder.build())
    }

    fn create_store(&self) -> Result<Store<PluginCtx>> {
        let wasi_ctx = self.build_wasi_ctx()?;
        Ok(Store::new(
            &self.engine,
            PluginCtx::new(wasi_ctx, self.app_handle.clone()),
        ))
    }

    fn build_linker(&self) -> Result<Linker<PluginCtx>> {
        let mut linker = Linker::new(&self.engine);
        p2::add_to_linker_async(&mut linker)
            .context("Failed to register the WASI interface with Linker")?;

        wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
            .context("Failed to register wasi-http with Linker")?;

        PsysWorld::add_to_linker::<PluginCtx, PluginCtx>(&mut linker, |ctx| ctx)
            .context("Failed to register the plugin host interface")?;

        Ok(linker)
    }

    pub async fn run(&self) -> Result<()> {
        log::info!("Creating store for plugin {}...", self.name.clone());
        let mut store = self.create_store()?;
        log::info!("Building linker for plugin {}...", self.name.clone());
        let linker = self.build_linker()?;

        log::info!("Instantiating world for plugin {}...", self.name.clone());
        let instance = PsysWorld::instantiate_async(&mut store, &self.component, &linker)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to instantiate plugin component. detail: {}",
                    e.to_string()
                )
            })?;

        log::info!("Calling on_load for plugin {}...", self.name.clone());
        let lifecycle = instance.astrobox_psys_plugin_lifecycle();
        lifecycle
            .call_on_load(&mut store)
            .await
            .context("Failed to execute the plugin on-load callback")?;

        Ok(())
    }
}

pub struct Plugin {
    pub path: PathBuf,
    pub manifest: PluginManifest,
    pub runtime: PluginRuntime,
    pub data: PluginData,
    pub state: PluginState,
}

impl Plugin {
    pub fn load(path: PathBuf, app_handle: AppHandle) -> Result<Self> {
        if !path.is_dir() {
            return Err(corelib::anyhow_site!(
                "Invalid plugin path: {}",
                path.display()
            ));
        }

        let manifest = PluginManifest::load_from_dir(&path)?;

        log::info!(
            "Initializing wasi runtime for plugin {}...",
            manifest.clone().name
        );
        let runtime = PluginRuntime::initialise(&path, &manifest, app_handle)?;

        Ok(Self {
            path,
            manifest,
            runtime,
            data: PluginData::default(),
            state: PluginState::default(),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.runtime.run().await?;
        self.state.disabled = false;
        self.state.loaded = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state.disabled = true;
        self.state.loaded = false;
    }
}
