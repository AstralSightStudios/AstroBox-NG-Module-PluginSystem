use crate::bindings::astrobox::psys_host;
use anyhow::{Context, Error};
use frontbridge::invoke_frontend;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

const FRONT_LANGUAGE_METHOD: &str = "host/os/astrobox_language";
const FRONT_APPEARANCE_METHOD: &str = "host/os/appearance";

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

    fn astrobox_language<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let future = accessor.with(|mut access| {
            let app_handle = app_handle.clone();
            FutureReader::new(instance, &mut access, async move {
                let language: String = invoke_frontend(&app_handle, FRONT_LANGUAGE_METHOD, ())
                    .await
                    .context("invoke frontend astrobox_language")?;
                Ok::<HostString, Error>(language.into())
            })
        });
        async move { future }
    }

    fn appearance<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostString>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let future = accessor.with(|mut access| {
            let app_handle = app_handle.clone();
            FutureReader::new(instance, &mut access, async move {
                let appearance: String =
                    invoke_frontend(&app_handle, FRONT_APPEARANCE_METHOD, ())
                        .await
                        .context("invoke frontend appearance")?;
                Ok::<HostString, Error>(appearance.into())
            })
        });
        async move { future }
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
