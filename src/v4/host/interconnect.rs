//! Level 4 的互联消息接口。业务逻辑与 Level 2/3 共用同一份实现。

use log::error;
use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::interconnect::send_qaic_message_impl;
use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::interconnect;
use crate::v4::ctx::PluginCtxV4;

impl interconnect::Host for PluginCtxV4 {}

impl interconnect::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn send_qaic_message(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        pkg_name: String,
        data: String,
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
            "addr": device_addr.clone(),
            "pkgName": pkg_name.clone(),
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "interconnect", params)
            .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match send_qaic_message_impl(device_addr, pkg_name, data.into_bytes()).await {
            Ok(()) => Ok(Ok(())),
            Err(err) => {
                error!("Failed to send QAIC message to package: {err:?}");
                Ok(Err(err.to_string()))
            }
        }
    }
}
