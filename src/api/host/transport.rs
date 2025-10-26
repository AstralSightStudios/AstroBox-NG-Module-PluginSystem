use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, HostVec, PluginCtx};

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

    fn request<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        data: HostVec<u8>,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<HostVec<u8>, ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = device_addr;
                let _ = data;
                Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Ok(HostVec::new()))
            })
        });
        async move { future }
    }

}
