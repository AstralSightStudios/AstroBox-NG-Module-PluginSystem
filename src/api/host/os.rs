use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

impl psys_host::os::Host for PluginCtx {}

impl psys_host::os::HostWithStore for PluginCtx {
    fn arch<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        make_string_future(accessor, || std::env::consts::ARCH.to_string())
    }

    fn hostname<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        make_string_future(accessor, || {
            whoami::fallible::hostname().unwrap_or_else(|_| "unknown-host".to_string())
        })
    }

    fn locale<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        make_string_future(accessor, || {
            sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string())
        })
    }

    fn platform<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        make_string_future(accessor, || os_info::get().os_type().to_string())
    }

    fn version<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        make_string_future(accessor, || os_info::get().version().to_string())
    }
}

fn make_string_future<T, F>(
    accessor: &Accessor<T, PluginCtx>,
    producer: F,
) -> impl core::future::Future<Output = FutureReader<HostString>> + Send
where
    F: FnOnce() -> String + Send + 'static,
{
    let instance = accessor.instance();
    let future = accessor.with(|mut access| {
        FutureReader::new(instance, &mut access, async move {
            Ok::<HostString, Error>(producer().into())
        })
    });
    async move { future }
}
