use crate::bindings::astrobox::psys_host;
use anyhow::{Context, Error};
use chrono::Local;
use frontbridge::invoke_frontend;
use wasmtime::component::{Access, FutureReader};

use super::{AccessExt, HostString, PluginCtx};

const FRONT_LANGUAGE_METHOD: &str = "host/os/astrobox_language";
const FRONT_APPEARANCE_METHOD: &str = "host/os/appearance";

impl psys_host::os::Host for PluginCtx {}

impl<T> psys_host::os::HostWithStore<T> for PluginCtx {
    fn arch(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut accessor, || std::env::consts::ARCH.to_string())
    }

    fn hostname(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut accessor, || {
            whoami::fallible::hostname().unwrap_or_else(|_| "unknown-host".to_string())
        })
    }

    fn locale(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut accessor, || {
            sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string())
        })
    }

    fn platform(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut accessor, || os_info::get().os_type().to_string())
    }

    fn version(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut accessor, || os_info::get().version().to_string())
    }

    fn astrobox_language(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let future = accessor.with(|mut access| {
            let app_handle = app_handle.clone();
            crate::api::host::new_future_reader!(&mut access, async move {
                let language: String = invoke_frontend(&app_handle, FRONT_LANGUAGE_METHOD, ())
                    .await
                    .context("invoke frontend astrobox_language")?;
                Ok::<HostString, Error>(language.into())
            })
        });
        future
    }

    fn appearance(mut accessor: Access<'_, T, Self>) -> FutureReader<HostString> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let future = accessor.with(|mut access| {
            let app_handle = app_handle.clone();
            crate::api::host::new_future_reader!(&mut access, async move {
                let appearance: String = invoke_frontend(&app_handle, FRONT_APPEARANCE_METHOD, ())
                    .await
                    .context("invoke frontend appearance")?;
                Ok::<HostString, Error>(appearance.into())
            })
        });
        future
    }

    fn timezone_offset_minutes(mut accessor: Access<'_, T, Self>) -> FutureReader<i32> {
        let future = accessor.with(|mut access| {
            crate::api::host::new_future_reader!(&mut access, async move {
                let offset_seconds = Local::now().offset().local_minus_utc();
                Ok::<i32, Error>(offset_seconds / 60)
            })
        });
        future
    }
}

fn make_string_future<T, F>(
    accessor: &mut Access<'_, T, PluginCtx>,
    producer: F,
) -> FutureReader<HostString>
where
    F: FnOnce() -> String + Send + 'static,
{
    crate::api::host::new_future_reader!(accessor, async move {
        Ok::<HostString, Error>(producer().into())
    })
}
