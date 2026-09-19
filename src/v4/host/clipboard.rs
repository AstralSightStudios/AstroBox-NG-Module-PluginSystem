//! Level 4 的剪贴板接口。读写分别独立鉴权，与 Level 2/3 一致。

use serde_json::json;
use tauri_plugin_clipboard_manager::ClipboardExt;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::clipboard;
use crate::v4::ctx::PluginCtxV4;

const READ_PERMISSION: &str = "clipboard.read";
const WRITE_PERMISSION: &str = "clipboard.write";

impl clipboard::Host for PluginCtxV4 {}

impl clipboard::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn read_text(
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
            READ_PERMISSION,
            json!({ "plugin": plugin_name }),
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match app_handle.clipboard().read_text() {
            Ok(content) => Ok(Ok(content)),
            Err(err) => {
                log::warn!("[plugin:{}] clipboard read_text failed: {err}", plugin_name);
                Ok(Err(err.to_string()))
            }
        }
    }

    async fn write_text(
        accessor: &Accessor<PluginCtxV4, Self>,
        text: String,
    ) -> wasmtime::Result<Result<(), String>> {
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
            WRITE_PERMISSION,
            json!({ "plugin": plugin_name }),
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        match app_handle.clipboard().write_text(text) {
            Ok(()) => Ok(Ok(())),
            Err(err) => {
                log::warn!(
                    "[plugin:{}] clipboard write_text failed: {err}",
                    plugin_name
                );
                Ok(Err(err.to_string()))
            }
        }
    }
}
