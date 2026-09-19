//! Level 4 的 HTTP 服务器接口。

use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::http_server;
use crate::v4::ctx::PluginCtxV4;

/// 起回环服务器所需的权限。
const PERMISSION: &str = "http-server";
/// 额外绑到所有网卡（局域网可见）所需的权限。
const LAN_PERMISSION: &str = "http-server.lan";

impl http_server::Host for PluginCtxV4 {
    fn list_servers(&mut self) -> wasmtime::Result<Vec<http_server::ServerInfo>> {
        // 只读本地登记表，不会等待，所以是同步接口。
        Ok(self.http_servers().list())
    }
}

impl http_server::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn start(
        accessor: &Accessor<PluginCtxV4, Self>,
        options: http_server::ServerOptions,
    ) -> wasmtime::Result<Result<http_server::ServerInfo, String>> {
        let (app_handle, plugin_name, permissions, registry, dispatch_tx) =
            accessor.with(|mut access| {
                let ctx = access.get();
                (
                    ctx.app_handle(),
                    ctx.plugin_name().to_string(),
                    ctx.permissions(),
                    ctx.http_servers(),
                    ctx.http_dispatch_tx(),
                )
            });

        let params = json!({
            "plugin": plugin_name,
            "port": options.port,
            "bindAllInterfaces": options.bind_all_interfaces,
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), PERMISSION, params).await {
            return Ok(Err("permission denied".to_string()));
        }

        // 绑到所有网卡意味着同一网络里的任何设备都能访问，单独再要一次授权。
        if options.bind_all_interfaces {
            let lan_params = json!({
                "plugin": plugin_name,
                "port": options.port,
            });
            if !check_permission_declared(
                &app_handle,
                permissions.as_ref(),
                LAN_PERMISSION,
                lan_params,
            )
            .await
            {
                return Ok(Err(
                    "permission denied for binding all interfaces".to_string()
                ));
            }
        }

        Ok(registry.start(plugin_name, options, dispatch_tx).await)
    }

    async fn stop(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
    ) -> wasmtime::Result<Result<(), String>> {
        let registry = accessor.with(|mut access| access.get().http_servers());
        Ok(registry.stop(id).await)
    }
}
