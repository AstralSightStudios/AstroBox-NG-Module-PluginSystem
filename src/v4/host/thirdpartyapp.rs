//! Level 4 的快应用接口。

use log::error;
use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::api::host::thirdpartyapp::{
    fingerprint_to_bytes, get_thirdparty_app_list_raw, launch_qa_raw,
};
use crate::v4::bindings::astrobox::psys_host_v4::thirdpartyapp;
use crate::v4::ctx::PluginCtxV4;

impl thirdpartyapp::Host for PluginCtxV4 {}

impl thirdpartyapp::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn launch_qa(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        app_info: thirdpartyapp::AppInfo,
        page_name: String,
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
            "pkgName": app_info.package_name.clone(),
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "thirdpartyapp", params)
            .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        let fingerprint = match fingerprint_to_bytes(app_info.fingerprint) {
            Ok(bytes) => bytes,
            Err(err) => return Ok(Err(err.to_string())),
        };

        match launch_qa_raw(addr, app_info.package_name, fingerprint, page_name).await {
            Ok(()) => Ok(Ok(())),
            Err(err) => {
                error!("Failed to launch third-party app: {err:?}");
                Ok(Err(err.to_string()))
            }
        }
    }

    async fn get_thirdparty_app_list(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
    ) -> wasmtime::Result<Result<Vec<thirdpartyapp::AppInfo>, String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        let params = json!({ "plugin": plugin_name, "addr": addr.clone() });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "thirdpartyapp", params)
            .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match get_thirdparty_app_list_raw(addr).await {
            Ok(list) => Ok(Ok(list
                .into_iter()
                .map(|item| thirdpartyapp::AppInfo {
                    package_name: item.package_name,
                    fingerprint: item.fingerprint.into_iter().map(u32::from).collect(),
                    version_code: item.version_code,
                    can_remove: item.can_remove,
                    app_name: item.app_name,
                })
                .collect())),
            Err(err) => {
                error!("Failed to fetch third-party app list: {err:?}");
                Ok(Err(err.to_string()))
            }
        }
    }
}
