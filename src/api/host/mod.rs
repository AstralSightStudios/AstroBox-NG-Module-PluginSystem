use std::sync::Arc;
use std::path::PathBuf;

use tauri::AppHandle;
use wasmtime::component::ResourceTable;
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
}

impl PluginCtx {
    pub fn new(
        wasi_ctx: WasiCtx,
        app_handle: AppHandle,
        plugin_root: PathBuf,
        plugin_name: String,
        register_state: Arc<PluginRegisterState>,
        permissions: Arc<Vec<String>>,
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

    pub(crate) fn permissions(&self) -> Arc<Vec<String>> {
        Arc::clone(&self.permissions)
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

mod device;
mod dialog;
mod event;
mod interconnect;
mod os;
mod permission;
mod queue;
mod register;
mod timer;
mod thirdpartyapp;
mod transport;
pub mod ui;
