use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;
use wasmtime::component::ResourceTable;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::plugin::PluginRegisterState;

pub(crate) type HostVec<T> = wasmtime::component::__internal::Vec<T>;
pub(crate) type HostString = wasmtime::component::__internal::String;

pub struct PluginCtx {
    table: ResourceTable,
    wasi_ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    app_handle: AppHandle,
    plugin_root: PathBuf,
    register_state: Arc<PluginRegisterState>,
    plugin_name: String,
    permissions: Arc<Vec<String>>,
    runtime_generation: u64,
    store_limits: StoreLimits,
}

impl PluginCtx {
    pub fn new(
        wasi_ctx: WasiCtx,
        app_handle: AppHandle,
        plugin_root: PathBuf,
        plugin_name: String,
        register_state: Arc<PluginRegisterState>,
        permissions: Arc<Vec<String>>,
        runtime_generation: u64,
    ) -> Self {
        Self {
            table: ResourceTable::new(),
            wasi_ctx,
            http_ctx: WasiHttpCtx::new(),
            app_handle,
            plugin_root,
            register_state,
            plugin_name,
            permissions,
            runtime_generation,
            store_limits: StoreLimitsBuilder::new()
                .memory_size(256 * 1024 * 1024)
                .table_elements(100_000)
                .instances(256)
                .tables(256)
                .memories(256)
                .build(),
        }
    }

    pub(crate) fn app_handle(&self) -> AppHandle {
        self.app_handle.clone()
    }

    pub(crate) fn register_state(&self) -> Arc<PluginRegisterState> {
        Arc::clone(&self.register_state)
    }

    pub(crate) fn plugin_name(&self) -> &str {
        self.plugin_name.as_str()
    }

    pub(crate) fn plugin_root(&self) -> &PathBuf {
        &self.plugin_root
    }

    pub(crate) fn runtime_generation(&self) -> u64 {
        self.runtime_generation
    }

    pub(crate) fn permissions(&self) -> Arc<Vec<String>> {
        Arc::clone(&self.permissions)
    }

    pub(crate) fn store_limits(&mut self) -> &mut StoreLimits {
        &mut self.store_limits
    }
}

impl WasiView for PluginCtx {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for PluginCtx {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http_ctx
    }
}

impl wasmtime::component::HasData for PluginCtx {
    type Data<'a> = &'a mut PluginCtx;
}

mod clipboard;
mod device;
pub(crate) mod dialog;
mod event;
mod i18n;
mod interconnect;
mod os;
mod permission;
mod provider_callback;
mod queue;
mod register;
mod thirdpartyapp;
mod timer;
mod transport;
pub mod ui;
pub mod v3;
