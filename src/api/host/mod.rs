use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};

use tauri::AppHandle;
use wasmtime::AsContextMut;
use wasmtime::StoreContextMut;
use wasmtime::component::{Access, FutureProducer, HasData, ResourceTable};
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p2::{WasiHttpCtxView, WasiHttpView},
};

use crate::plugin::PluginRegisterState;

pub(crate) type HostVec<T> = wasmtime::component::__internal::Vec<T>;
pub(crate) type HostString = wasmtime::component::__internal::String;

pub(crate) trait AccessExt<T, D>
where
    T: 'static,
    D: HasData + ?Sized,
{
    fn with<R>(&mut self, fun: impl FnOnce(Access<'_, T, D>) -> R) -> R;
}

impl<'a, T, D> AccessExt<T, D> for Access<'a, T, D>
where
    T: 'static,
    D: HasData + ?Sized,
{
    fn with<R>(&mut self, fun: impl FnOnce(Access<'_, T, D>) -> R) -> R {
        let getter = self.getter();
        let store = self.as_context_mut();
        fun(Access::new(store, getter))
    }
}

pub(crate) struct AnyhowFuture<F>(pub F);

impl<D, F, T> FutureProducer<D> for AnyhowFuture<F>
where
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    type Item = T;

    fn poll_produce(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        _: StoreContextMut<D>,
        finish: bool,
    ) -> Poll<wasmtime::Result<Option<Self::Item>>> {
        let future = unsafe { self.map_unchecked_mut(|this| &mut this.0) };
        match future.poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(Some(value))),
            Poll::Ready(Err(err)) => Poll::Ready(Err(wasmtime::Error::from_anyhow(err))),
            Poll::Pending if finish => Poll::Ready(Ok(None)),
            Poll::Pending => Poll::Pending,
        }
    }
}

macro_rules! new_future_reader {
    ($access:expr, $future:expr) => {
        wasmtime::component::FutureReader::new($access, $crate::api::host::AnyhowFuture($future))
            .expect("failed to create host future reader")
    };
}

pub(crate) use new_future_reader;

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
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http_ctx,
            table: &mut self.table,
            hooks: Default::default(),
        }
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
