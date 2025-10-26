use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

type HostVec<T> = wasmtime::component::__internal::Vec<T>;
type HostString = wasmtime::component::__internal::String;

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

impl psys_host::os::Host for PluginCtx {}

impl psys_host::os::HostWithStore for PluginCtx {
    fn arch<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostString, Error>(HostString::default())
            })
        });
        async move { future }
    }

    fn hostname<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostString, Error>(HostString::default())
            })
        });
        async move { future }
    }

    fn locale<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostString, Error>(HostString::default())
            })
        });
        async move { future }
    }

    fn platform<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostString, Error>(HostString::default())
            })
        });
        async move { future }
    }

    fn version<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostString, Error>(HostString::default())
            })
        });
        async move { future }
    }
}

impl psys_host::transport::Host for PluginCtx {
    fn to_json(
        &mut self,
        _protocol: psys_host::transport::Protocol,
        _data: HostVec<u8>,
    ) -> HostString {
        HostString::default()
    }

    fn from_json(
        &mut self,
        _protocol: psys_host::transport::Protocol,
        _data: HostString,
    ) -> core::result::Result<HostVec<u8>, ()> {
        Ok(HostVec::new())
    }
}

impl psys_host::transport::HostWithStore for PluginCtx {
    fn send<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        data: HostVec<u8>,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = device_addr;
                let _ = data;
                Ok::<(), Error>(())
            })
        });
        async move { future }
    }
}

impl psys_host::device::Host for PluginCtx {}

impl psys_host::device::HostWithStore for PluginCtx {
    fn get_device_list<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostVec<psys_host::device::DeviceInfo>>> + Send
    {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(HostVec::new())
            })
        });
        async move { future }
    }

    fn get_connected_device_list<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostVec<psys_host::device::DeviceInfo>>> + Send
    {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(HostVec::new())
            })
        });
        async move { future }
    }

    fn disconnect_device<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = device_addr;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }
}

impl psys_host::register::Host for PluginCtx {}

impl psys_host::register::HostWithStore for PluginCtx {
    fn register_transport_recv<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        filter: psys_host::register::TransportRecvFiler,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                let _ = filter;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_interconnect_recv<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        pkg_name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                let _ = pkg_name;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_deeplink_action<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_provider<T>(
        accessor: &Accessor<T, Self>,
        name: HostString,
        provider_type: psys_host::register::ProviderType,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = name;
                let _ = provider_type;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }
}

impl psys_host::event::Host for PluginCtx {
    fn send_event(&mut self, _event_name: HostString, _payload: HostString) {}
}

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        _res_type: psys_host::queue::ResourceType,
        _file_path: HostString,
    ) {
    }
}

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::interconnect::HostWithStore for PluginCtx {
    fn send_qaic_message<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        pkg_name: HostString,
        data: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = device_addr;
                let _ = pkg_name;
                let _ = data;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }
}

impl psys_host::picker::Host for PluginCtx {}

impl psys_host::picker::HostWithStore for PluginCtx {
    fn pick_file<T>(
        accessor: &Accessor<T, Self>,
        config: psys_host::picker::PickConfig,
        filter: psys_host::picker::FilterConfig,
    ) -> impl core::future::Future<Output = FutureReader<psys_host::picker::PickResult>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = config;
                let _ = filter;
                Ok::<psys_host::picker::PickResult, Error>(psys_host::picker::PickResult {
                    name: HostString::default(),
                    data: HostVec::new(),
                })
            })
        });
        async move { future }
    }
}

impl psys_host::thirdpartyapp::Host for PluginCtx {}

impl psys_host::thirdpartyapp::HostWithStore for PluginCtx {
    fn launch_qa<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        app_info: psys_host::thirdpartyapp::AppInfo,
        page_name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                let _ = app_info;
                let _ = page_name;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn get_thirdparty_app_list<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
    ) -> impl core::future::Future<
        Output = FutureReader<core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>>,
    > + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                Ok::<
                    core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                    Error,
                >(Ok(HostVec::new()))
            })
        });
        async move { future }
    }
}
