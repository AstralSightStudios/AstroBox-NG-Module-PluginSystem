use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

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
