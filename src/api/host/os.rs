use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

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
