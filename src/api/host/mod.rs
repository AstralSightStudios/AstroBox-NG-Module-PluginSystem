use tauri::AppHandle;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

pub(crate) type HostVec<T> = wasmtime::component::__internal::Vec<T>;
pub(crate) type HostString = wasmtime::component::__internal::String;

pub struct PluginCtx {
    table: ResourceTable,
    wasi_ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    app_handle: AppHandle,
}

impl PluginCtx {
    pub fn new(wasi_ctx: WasiCtx, app_handle: AppHandle) -> Self {
        Self {
            table: ResourceTable::new(),
            wasi_ctx,
            http_ctx: WasiHttpCtx::new(),
            app_handle,
        }
    }

    pub(crate) fn app_handle(&self) -> AppHandle {
        self.app_handle.clone()
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
mod event;
mod interconnect;
mod os;
mod picker;
mod queue;
mod register;
mod thirdpartyapp;
mod transport;
mod ui;
