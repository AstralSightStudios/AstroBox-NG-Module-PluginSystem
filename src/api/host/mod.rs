use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};

use tauri::AppHandle;
use wasmtime::StoreContextMut;
use wasmtime::component::{FutureProducer, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p2::{WasiHttpCtxView, WasiHttpView},
};

use crate::plugin::PluginRegisterState;

pub(crate) type HostVec<T> = wasmtime::component::__internal::Vec<T>;
pub(crate) type HostString = wasmtime::component::__internal::String;

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

mod device;
mod dialog;
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
