use crate::astrobox::psys_host;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

pub struct PluginCtx {
    table: ResourceTable,
    wasi_ctx: WasiCtx,
}

impl PluginCtx {
    pub fn new(wasi_ctx: WasiCtx) -> Self {
        Self {
            table: ResourceTable::new(),
            wasi_ctx,
        }
    }

    pub fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    pub fn wasi_ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

impl wasmtime::component::HasData for PluginCtx {
    type Data<'a> = &'a mut PluginCtx;
}

impl WasiView for PluginCtx {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl psys_host::config::Host for PluginCtx {
    fn read(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<(
        wasmtime::component::__internal::String,
        wasmtime::component::__internal::String,
    )> {
        todo!()
    }

    fn write(
        &mut self,
        content: wasmtime::component::__internal::Vec<(
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )>,
    ) -> () {
        todo!()
    }
}

impl psys_host::debug::Host for PluginCtx {}

impl psys_host::debug::HostWithStore for PluginCtx {
    fn send_raw<T>(
        _accessor: &wasmtime::component::Accessor<T, Self>,
        _data: wasmtime::component::__internal::Vec<u8>,
    ) -> impl core::future::Future<Output = ()> + Send {
        async move { todo!() }
    }
}

impl psys_host::device::Host for PluginCtx {
    fn get_device_list(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<psys_host::device::DeviceInfo> {
        todo!()
    }

    fn get_connected_device_list(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<psys_host::device::DeviceInfo> {
        todo!()
    }
}

impl psys_host::device::HostWithStore for PluginCtx {
    fn disconnect_device<T>(
        _accessor: &wasmtime::component::Accessor<T, Self>,
        _device: psys_host::device::DeviceInfo,
    ) -> impl core::future::Future<Output = ()> + Send {
        async move { todo!() }
    }
}

impl psys_host::event::Host for PluginCtx {
    fn send_event(
        &mut self,
        event_name: wasmtime::component::__internal::String,
        payload: wasmtime::component::__internal::String,
    ) -> () {
        todo!()
    }
}

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::interconnect::HostWithStore for PluginCtx {
    fn send_qaic_message<T>(
        _accessor: &wasmtime::component::Accessor<T, Self>,
        _pkg_name: wasmtime::component::__internal::String,
        _data: wasmtime::component::__internal::String,
    ) -> impl core::future::Future<Output = ()> + Send {
        async move { todo!() }
    }
}

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        res_type: psys_host::queue::ResourceType,
        file_path: wasmtime::component::__internal::String,
    ) -> () {
        todo!()
    }
}

impl psys_host::thirdpartyapp::Host for PluginCtx {}

impl psys_host::thirdpartyapp::HostWithStore for PluginCtx {
    fn launch_qa<T>(
        _accessor: &wasmtime::component::Accessor<T, Self>,
        _app_info: psys_host::thirdpartyapp::AppInfo,
        _page_name: wasmtime::component::__internal::String,
    ) -> impl core::future::Future<Output = ()> + Send {
        async move { todo!() }
    }

    fn get_thirdparty_app_list<T>(
        _accessor: &wasmtime::component::Accessor<T, Self>,
    ) -> impl core::future::Future<
        Output = wasmtime::component::__internal::Vec<psys_host::thirdpartyapp::AppInfo>,
    > + Send {
        async move { todo!() }
    }
}
