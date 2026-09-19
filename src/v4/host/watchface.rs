//! Level 4 的表盘接口。

use log::error;
use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::api::host::v3::watchface::{get_watchface_list_raw, set_current_watchface_impl};
use crate::v4::bindings::astrobox::psys_host_v4::watchface;
use crate::v4::ctx::PluginCtxV4;

impl watchface::Host for PluginCtxV4 {}

impl watchface::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn get_watchface_list(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
    ) -> wasmtime::Result<Result<Vec<watchface::WatchfaceInfo>, String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        let params = json!({ "plugin": plugin_name, "addr": addr.clone() });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "watchface", params).await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match get_watchface_list_raw(addr).await {
            Ok(list) => Ok(Ok(list
                .into_iter()
                .map(|item| watchface::WatchfaceInfo {
                    id: item.id,
                    name: item.name,
                    is_current: item.is_current,
                })
                .collect())),
            Err(err) => {
                error!("Failed to fetch watchface list: {err:?}");
                Ok(Err(err.to_string()))
            }
        }
    }

    async fn set_current_watchface(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        watchface_id: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        let params = json!({
            "plugin": plugin_name,
            "addr": addr.clone(),
            "watchfaceId": watchface_id.clone(),
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "watchface", params).await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match set_current_watchface_impl(addr, watchface_id).await {
            Ok(()) => Ok(Ok(())),
            Err(err) => {
                error!("Failed to set current watchface: {err:?}");
                Ok(Err(err.to_string()))
            }
        }
    }
}
