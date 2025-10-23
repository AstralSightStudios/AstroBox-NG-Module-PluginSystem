use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

type HostVec<T> = wasmtime::component::__internal::Vec<T>;

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
}

impl WasiView for PluginCtx {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl wasmtime::component::HasData for PluginCtx {
    type Data<'a> = &'a mut PluginCtx;
}

impl psys_host::config::Host for PluginCtx {
    fn read(&mut self) -> HostVec<(wasmtime::component::__internal::String, wasmtime::component::__internal::String)>
    {
        HostVec::new()
    }

    fn write(
        &mut self,
        _content: HostVec<(
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )>,
    ) {
    }
}

impl psys_host::debug::Host for PluginCtx {}

impl psys_host::debug::HostWithStore for PluginCtx {
    fn send_raw<T>(
        accessor: &Accessor<T, Self>,
        data: HostVec<u8>,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = data;
                Ok::<(), Error>(())
            })
        });
        async move { future }
    }
}

impl psys_host::device::Host for PluginCtx {
    fn get_device_list(&mut self) -> HostVec<psys_host::device::DeviceInfo> {
        HostVec::new()
    }

    fn get_connected_device_list(&mut self) -> HostVec<psys_host::device::DeviceInfo> {
        HostVec::new()
    }
}

impl psys_host::device::HostWithStore for PluginCtx {
    fn disconnect_device<T>(
        accessor: &Accessor<T, Self>,
        _device: psys_host::device::DeviceInfo,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move { Ok::<(), Error>(()) })
        });
        async move { future }
    }
}

impl psys_host::event::Host for PluginCtx {
    fn send_event(
        &mut self,
        _event_name: wasmtime::component::__internal::String,
        _payload: wasmtime::component::__internal::String,
    ) {
    }
}

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::interconnect::HostWithStore for PluginCtx {
    fn send_qaic_message<T>(
        accessor: &Accessor<T, Self>,
        _pkg_name: wasmtime::component::__internal::String,
        _data: wasmtime::component::__internal::String,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move { Ok::<(), Error>(()) })
        });
        async move { future }
    }
}

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        _res_type: psys_host::queue::ResourceType,
        _file_path: wasmtime::component::__internal::String,
    ) {
    }
}

impl psys_host::thirdpartyapp::Host for PluginCtx {}

impl psys_host::thirdpartyapp::HostWithStore for PluginCtx {
    fn launch_qa<T>(
        accessor: &Accessor<T, Self>,
        _app_info: psys_host::thirdpartyapp::AppInfo,
        _page_name: wasmtime::component::__internal::String,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move { Ok::<(), Error>(()) })
        });
        async move { future }
    }

    fn get_thirdparty_app_list<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<
        Output = FutureReader<HostVec<psys_host::thirdpartyapp::AppInfo>>,
    > + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostVec<psys_host::thirdpartyapp::AppInfo>, Error>(HostVec::new())
            })
        });
        async move { future }
    }
}
