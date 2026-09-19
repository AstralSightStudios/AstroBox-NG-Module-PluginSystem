//! Level 4 的 os 接口。逻辑与 Level 2/3 相同，只是不必再包 `FutureReader`。

use anyhow::Context;
use chrono::Local;
use serde_json::json;
use frontbridge::invoke_frontend;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::os;
use crate::v4::ctx::PluginCtxV4;

const FRONT_LANGUAGE_METHOD: &str = "host/os/astrobox_language";
const FRONT_APPEARANCE_METHOD: &str = "host/os/appearance";

impl os::Host for PluginCtxV4 {}

impl os::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn device_id(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<Result<String, String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });
        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "os.device-id",
            json!({ "plugin": plugin_name }),
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }
        Ok(crate::v4::identity::device_id(&app_handle)
            .await
            .map_err(|_| "could not persist or read host device identifier".to_string()))
    }

    async fn arch(_accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        Ok(std::env::consts::ARCH.to_string())
    }

    async fn hostname(_accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        Ok(whoami::fallible::hostname().unwrap_or_else(|_| "unknown-host".to_string()))
    }

    async fn locale(_accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        Ok(sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string()))
    }

    async fn platform(_accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        Ok(os_info::get().os_type().to_string())
    }

    async fn version(_accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        Ok(os_info::get().version().to_string())
    }

    async fn astrobox_language(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<String> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let language: String = invoke_frontend(&app_handle, FRONT_LANGUAGE_METHOD, ())
            .await
            .context("invoke frontend astrobox_language")
            .map_err(wasmtime::Error::from_anyhow)?;
        Ok(language)
    }

    async fn appearance(accessor: &Accessor<PluginCtxV4, Self>) -> wasmtime::Result<String> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let appearance: String = invoke_frontend(&app_handle, FRONT_APPEARANCE_METHOD, ())
            .await
            .context("invoke frontend appearance")
            .map_err(wasmtime::Error::from_anyhow)?;
        Ok(appearance)
    }

    async fn timezone_offset_minutes(
        _accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<i32> {
        Ok(Local::now().offset().local_minus_utc() / 60)
    }
}
