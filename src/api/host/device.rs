use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, HostVec, PluginCtx};

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
