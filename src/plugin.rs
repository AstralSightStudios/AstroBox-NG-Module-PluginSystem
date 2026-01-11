use std::collections::{HashMap, hash_map::DefaultHasher};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder, p2};

use crate::api::host::PluginCtx;
use crate::bindings::{PsysWorld, astrobox::psys_host, exports::astrobox::psys_plugin};
use crate::manifest::PluginManifest;
use crate::{PLUGINSYSTEM_PROGRESS_EVENT, PluginSystemProgressPayload};

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

#[derive(Debug, Clone)]
pub struct TransportRecvRegistration {
    pub addr: String,
    pub filter: psys_host::register::TransportRecvFiler,
}

#[derive(Debug, Clone)]
pub struct InterconnectRecvRegistration {
    pub addr: String,
    pub pkg_name: String,
}

#[derive(Debug, Clone)]
pub struct ProviderRegistration {
    pub name: String,
    pub provider_type: psys_host::register::ProviderType,
}

#[derive(Debug, Clone)]
pub struct CardRegistration {
    pub card_type: psys_host::register::CardType,
    pub id: String,
    pub name: String,
}

#[derive(Default)]
pub struct PluginRegisterState {
    transport_recv: Mutex<Vec<TransportRecvRegistration>>,
    interconnect_recv: Mutex<Vec<InterconnectRecvRegistration>>,
    providers: Mutex<Vec<ProviderRegistration>>,
    cards: Mutex<Vec<CardRegistration>>,
    deeplink_registered: Mutex<bool>,
}

impl PluginRegisterState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_transport_recv(&self, registration: TransportRecvRegistration) {
        self.transport_recv.lock().await.push(registration);
    }

    pub async fn register_interconnect_recv(&self, registration: InterconnectRecvRegistration) {
        self.interconnect_recv.lock().await.push(registration);
    }

    pub async fn register_provider(&self, registration: ProviderRegistration) {
        self.providers.lock().await.push(registration);
    }

    pub async fn register_card(&self, registration: CardRegistration) {
        self.cards.lock().await.push(registration);
    }

    pub async fn list_cards(&self) -> Vec<CardRegistration> {
        self.cards.lock().await.clone()
    }

    pub async fn try_register_deeplink(&self) -> bool {
        let mut guard = self.deeplink_registered.lock().await;
        if *guard {
            false
        } else {
            *guard = true;
            true
        }
    }
}

const PRECOMPILE_INDEX_FILE: &str = "precompiled-index.json";

#[derive(Default, Serialize, Deserialize)]
struct PrecompiledIndex {
    #[serde(default)]
    entries: HashMap<String, PrecompiledRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PrecompiledRecord {
    wasm_sha256: String,
    engine_hash: u64,
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
        format!("failed to select Wasmtime interpreter target `{pulley_triple}` for iOS with moving memories")
    })?;

    const RESERVE: u64 = 128 << 20; // 128 MiB

    config
        .memory_may_move(true)
        .memory_reservation(RESERVE)
        .memory_reservation_for_growth(RESERVE);

    log::info!(
        "Detected iOS runtime; Wasmtime configured for interpreter mode via target `{pulley_triple}` with moving memories"
    );

    Ok(())
}

#[cfg(not(target_os = "ios"))]
fn configure_engine(_config: &mut Config) -> Result<()> {
    Ok(())
}

impl PrecompiledIndex {
    fn load(root: &Path) -> Result<Self> {
        let path = root.join(PRECOMPILE_INDEX_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(&path)
            .with_context(|| format!("failed to read precompile index {}", path.display()))?;

        match serde_json::from_str::<Self>(&data) {
            Ok(index) => Ok(index),
            Err(err) => {
                log::warn!(
                    "Failed to parse precompile index {}: {err}; recreating index",
                    path.display()
                );
                Ok(Self::default())
            }
        }
    }

    fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(PRECOMPILE_INDEX_FILE);
        let data = serde_json::to_string_pretty(self)
            .context("failed to serialize precompile index into JSON")?;

        fs::write(&path, data)
            .with_context(|| format!("failed to persist precompile index to {}", path.display()))
    }
}

fn precompile_index_root(plugin_dir: &Path) -> PathBuf {
    plugin_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| plugin_dir.to_path_buf())
}

fn precompiled_artifact_path(entry_wasm: &Path) -> PathBuf {
    entry_wasm.with_extension("cwasm")
}

fn compute_wasm_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open wasm file for hashing {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to hash wasm file {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn engine_config_hash(engine: &Engine) -> u64 {
    let mut hasher = DefaultHasher::new();
    engine.precompile_compatibility_hash().hash(&mut hasher);
    hasher.finish()
}

fn ensure_precompiled_component(
    engine: &Engine,
    plugin_dir: &Path,
    manifest: &PluginManifest,
    entry_wasm: &Path,
) -> Result<PathBuf> {
    let root = precompile_index_root(plugin_dir);
    let mut index = PrecompiledIndex::load(&root)?;

    let wasm_hash = compute_wasm_hash(entry_wasm)?;
    let engine_hash = engine_config_hash(engine);
    let key = manifest.name.clone();
    let artifact_path = precompiled_artifact_path(entry_wasm);

    let entry = index.entries.get(&key);
    let needs_recompile = entry
        .map(|cached| cached.wasm_sha256 != wasm_hash || cached.engine_hash != engine_hash)
        .unwrap_or(true)
        || !artifact_path.is_file();

    if needs_recompile {
        log::info!(
            "Precompiling plugin {} wasm for faster startup...",
            manifest.name
        );
        let wasm_bytes = fs::read(entry_wasm).with_context(|| {
            format!(
                "failed to read plugin wasm component {}",
                entry_wasm.display()
            )
        })?;
        let compiled = engine.precompile_component(&wasm_bytes).with_context(|| {
            format!(
                "failed to precompile component for plugin {}",
                manifest.name
            )
        })?;

        fs::write(&artifact_path, compiled).with_context(|| {
            format!(
                "failed to write precompiled artifact for plugin {} at {}",
                manifest.name,
                artifact_path.display()
            )
        })?;

        index.entries.insert(
            key,
            PrecompiledRecord {
                wasm_sha256: wasm_hash,
                engine_hash,
            },
        );
        index.save(&root)?;
    }

    Ok(artifact_path)
}

pub(crate) fn purge_precompiled_component(
    plugin_dir: &Path,
    manifest: &PluginManifest,
) -> Result<()> {
    let root = precompile_index_root(plugin_dir);
    let entry_wasm = manifest.entry_wasm_path(plugin_dir);
    let artifact_path = precompiled_artifact_path(&entry_wasm);

    if artifact_path.exists() {
        if let Err(err) = fs::remove_file(&artifact_path) {
            log::warn!(
                "Failed to remove precompiled artifact {}: {err}",
                artifact_path.display()
            );
        }
    }

    let mut index = PrecompiledIndex::load(&root)?;
    if index.entries.remove(&manifest.name).is_some() {
        index.save(&root)?;
    }

    Ok(())
}

fn create_engine() -> Result<Engine> {
    let mut config = Config::default();
    configure_engine(&mut config)?;
    config
        .wasm_memory64(false)
        .wasm_component_model(true)
        .wasm_component_model_async(true)
        .async_support(true);

    Engine::new(&config).context("Failed to initialize the Wasmtime engine")
}

fn emit_pluginsystem_progress(
    app_handle: &AppHandle,
    plugin: &str,
    stage: &str,
    detail: Option<String>,
) {
    let payload = PluginSystemProgressPayload {
        plugin: plugin.to_string(),
        stage: stage.to_string(),
        detail,
    };
    if let Err(err) = app_handle.emit(PLUGINSYSTEM_PROGRESS_EVENT, &payload) {
        log::error!("Failed to emit plugin progress event: {err}");
    }
}

#[derive(Clone)]
pub struct PluginRuntime {
    name: String,
    engine: Engine,
    component: Component,
    plugin_root: PathBuf,
    app_handle: AppHandle,
    register_state: Arc<PluginRegisterState>,
    instance: Arc<Mutex<Option<PluginInstance>>>,
}

struct PluginInstance {
    store: Store<PluginCtx>,
    world: PsysWorld,
}

impl PluginRuntime {
    fn emit_progress(&self, stage: &str, detail: Option<String>) {
        emit_pluginsystem_progress(&self.app_handle, &self.name, stage, detail);
    }

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

        let plugin_name = manifest.name.clone();

        log::info!("Creating wasmtime engine for plugin {}...", plugin_name);
        let engine = create_engine()?;

        log::info!(
            "Ensuring precompiled component for plugin {}...",
            plugin_name
        );
        let artifact_path = ensure_precompiled_component(&engine, path, manifest, &entry_path)?;

        log::info!(
            "Loading precompiled component for plugin {}...",
            plugin_name
        );
        let component = unsafe {
            // SAFETY: `artifact_path` is produced via `Engine::precompile_component` with
            // the same engine configuration, satisfying Wasmtime's deserialize requirements.
            Component::deserialize_file(&engine, &artifact_path).with_context(|| {
                format!(
                    "Failed to load precompiled plugin component: {}",
                    artifact_path.display()
                )
            })?
        };

        Ok(Self {
            name: plugin_name,
            engine,
            component,
            plugin_root: path.to_path_buf(),
            app_handle,
            register_state: Arc::new(PluginRegisterState::new()),
            instance: Arc::new(Mutex::new(None)),
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
            PluginCtx::new(
                wasi_ctx,
                self.app_handle.clone(),
                self.name.clone(),
                Arc::clone(&self.register_state),
            ),
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
        self.emit_progress("create_store", None);
        let mut store = self.create_store()?;
        log::info!("Building linker for plugin {}...", self.name.clone());
        self.emit_progress("build_linker", None);
        let linker = self.build_linker()?;

        log::info!("Instantiating world for plugin {}...", self.name.clone());
        self.emit_progress("instantiate", None);
        {
            let mut guard = self.instance.lock().await;
            *guard = None;
        }
        let instance = PsysWorld::instantiate_async(&mut store, &self.component, &linker)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to instantiate plugin component. detail: {}",
                    e.to_string()
                )
            })?;

        log::info!("Calling on_load for plugin {}...", self.name.clone());
        self.emit_progress("on_load", None);
        let lifecycle = instance.astrobox_psys_plugin_lifecycle();
        lifecycle
            .call_on_load(&mut store)
            .await
            .context("Failed to execute the plugin on-load callback")?;

        {
            let mut guard = self.instance.lock().await;
            *guard = Some(PluginInstance { store, world: instance });
        }

        Ok(())
    }

    pub async fn dispatch_event(
        &self,
        event_type: psys_plugin::event::EventType,
        payload: String,
    ) -> Result<()> {
        let mut guard = self.instance.lock().await;
        let instance = guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Plugin '{}' instance is not initialized", self.name)
        })?;
        let event_iface = instance.world.astrobox_psys_plugin_event();
        let mut future = event_iface
            .call_on_event(&mut instance.store, event_type, payload.as_str())
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start the plugin on-event callback. detail: {}",
                    e.to_string()
                )
            })?;
        future.close(&mut instance.store);
        Ok(())
    }

    pub async fn dispatch_ui_render(&self, element_id: String) -> Result<()> {
        let mut guard = self.instance.lock().await;
        let instance = guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Plugin '{}' instance is not initialized", self.name)
        })?;
        let event_iface = instance.world.astrobox_psys_plugin_event();
        let mut future = event_iface
            .call_on_ui_render(&mut instance.store, element_id.as_str())
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start the plugin on-ui-render callback. detail: {}",
                    e.to_string()
                )
            })?;
        future.close(&mut instance.store);
        Ok(())
    }

    pub async fn dispatch_ui_event(
        &self,
        event_id: String,
        event: psys_host::ui::Event,
        payload: String,
    ) -> Result<()> {
        let mut guard = self.instance.lock().await;
        let instance = guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Plugin '{}' instance is not initialized", self.name)
        })?;
        let event_iface = instance.world.astrobox_psys_plugin_event();
        let mut future = event_iface
            .call_on_ui_event(
                &mut instance.store,
                event_id.as_str(),
                event,
                payload.as_str(),
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to start the plugin on-ui-event callback. detail: {}",
                    e.to_string()
                )
            })?;
        future.close(&mut instance.store);
        Ok(())
    }

    pub async fn dispatch_plugin_message(&self, payload: String) -> Result<()> {
        self.dispatch_event(psys_plugin::event::EventType::PluginMessage, payload)
            .await
    }

    pub async fn dispatch_interconnect_message(&self, payload: String) -> Result<()> {
        self.dispatch_event(
            psys_plugin::event::EventType::InterconnectMessage,
            payload,
        )
        .await
    }

    pub async fn matches_interconnect(&self, addr: &str, pkg_name: &str) -> bool {
        let registrations = self.register_state.interconnect_recv.lock().await;
        registrations
            .iter()
            .any(|reg| reg.addr == addr && reg.pkg_name == pkg_name)
    }

    pub async fn list_cards(&self) -> Vec<CardRegistration> {
        self.register_state.list_cards().await
    }

    pub async fn clear_instance(&self) {
        let mut guard = self.instance.lock().await;
        *guard = None;
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

    pub async fn stop(&mut self) {
        self.runtime.clear_instance().await;
        self.state.disabled = true;
        self.state.loaded = false;
    }
}
