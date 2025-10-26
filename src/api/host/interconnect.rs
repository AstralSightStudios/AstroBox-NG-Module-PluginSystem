use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

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
