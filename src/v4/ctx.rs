//! Level 4 的 Store 数据。
//!
//! 结构与 Level 2/3 的 `crate::api::host::PluginCtx` 对齐，但所有 wasmtime 类型
//! 都来自 48.x，因此必须是独立的一份，不能复用。
//!
//! 注意 `PluginRegisterState` 是**共享**的：注册表里存的是 Level 2/3 的 bindgen
//! 类型（`TransportRecvFiler` / `ProviderType` / `CardType`），Level 4 在注册时把
//! 自己的枚举转过去。这样 `PluginManager` 不用关心插件是哪个 API Level。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;
use wasmtime_v4 as wasmtime;

use wasmtime::component::ResourceTable;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi_http_v4::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};
use wasmtime_wasi_v4::{WasiCtx, WasiCtxView, WasiView};

use crate::plugin::PluginRegisterState;
use crate::v4::http_server::{HttpDispatch, HttpServerRegistry};
use tokio::sync::mpsc;

pub struct PluginCtxV4 {
    pub(crate) table: ResourceTable,
    wasi_ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    app_handle: AppHandle,
    plugin_root: PathBuf,
    register_state: Arc<PluginRegisterState>,
    plugin_name: String,
    permissions: Arc<Vec<String>>,
    runtime_generation: u64,
    store_limits: StoreLimits,
    http_servers: Arc<HttpServerRegistry>,
    http_dispatch_tx: mpsc::UnboundedSender<HttpDispatch>,
}

impl PluginCtxV4 {
    pub(crate) fn new(
        wasi_ctx: WasiCtx,
        app_handle: AppHandle,
        plugin_root: PathBuf,
        plugin_name: String,
        register_state: Arc<PluginRegisterState>,
        permissions: Arc<Vec<String>>,
        runtime_generation: u64,
        http_servers: Arc<HttpServerRegistry>,
        http_dispatch_tx: mpsc::UnboundedSender<HttpDispatch>,
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
            http_servers,
            http_dispatch_tx,
        }
    }

    pub(crate) fn http_servers(&self) -> Arc<HttpServerRegistry> {
        Arc::clone(&self.http_servers)
    }

    pub(crate) fn http_dispatch_tx(&self) -> mpsc::UnboundedSender<HttpDispatch> {
        self.http_dispatch_tx.clone()
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

    #[allow(dead_code)]
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

impl WasiView for PluginCtxV4 {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl WasiHttpView for PluginCtxV4 {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http_ctx,
            table: &mut self.table,
            hooks: wasmtime_wasi_http_v4::default_hooks(),
        }
    }
}

impl wasmtime::component::HasData for PluginCtxV4 {
    type Data<'a> = &'a mut PluginCtxV4;
}
