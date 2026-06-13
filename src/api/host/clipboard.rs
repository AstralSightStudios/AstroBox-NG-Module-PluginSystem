use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use serde_json::{Value, json};
use tauri_plugin_clipboard_manager::ClipboardExt;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx, permission::check_permission_declared};

const READ_PERMISSION: &str = "clipboard.read";
const WRITE_PERMISSION: &str = "clipboard.write";

impl psys_host::clipboard::Host for PluginCtx {}

impl psys_host::clipboard::HostWithStore for PluginCtx {
    fn read_text<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<HostString, ()>>> + Send
    {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    READ_PERMISSION,
                    clipboard_permission_params(&plugin_name),
                )
                .await
                {
                    return Ok::<core::result::Result<HostString, ()>, Error>(Err(()));
                }

                match app_handle.clipboard().read_text() {
                    Ok(content) => {
                        Ok::<core::result::Result<HostString, ()>, Error>(Ok(content.into()))
                    }
                    Err(err) => {
                        log::warn!("[plugin:{}] clipboard read_text failed: {err}", plugin_name);
                        Ok::<core::result::Result<HostString, ()>, Error>(Err(()))
                    }
                }
            })
        });
        async move { future }
    }

    fn write_text<T>(
        accessor: &Accessor<T, Self>,
        text: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let text = text.to_string();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    WRITE_PERMISSION,
                    clipboard_permission_params(&plugin_name),
                )
                .await
                {
                    return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                }

                match app_handle.clipboard().write_text(text) {
                    Ok(()) => Ok::<core::result::Result<(), ()>, Error>(Ok(())),
                    Err(err) => {
                        log::warn!(
                            "[plugin:{}] clipboard write_text failed: {err}",
                            plugin_name
                        );
                        Ok::<core::result::Result<(), ()>, Error>(Err(()))
                    }
                }
            })
        });
        async move { future }
    }
}

fn clipboard_permission_params(plugin_name: &str) -> Value {
    json!({ "plugin": plugin_name })
}

#[cfg(test)]
mod tests {
    use super::{READ_PERMISSION, WRITE_PERMISSION, clipboard_permission_params};

    #[test]
    fn clipboard_permissions_are_split_between_read_and_write() {
        assert_eq!(READ_PERMISSION, "clipboard.read");
        assert_eq!(WRITE_PERMISSION, "clipboard.write");
        assert_ne!(READ_PERMISSION, WRITE_PERMISSION);
    }

    #[test]
    fn clipboard_permission_params_include_plugin_name() {
        assert_eq!(
            clipboard_permission_params("demo-plugin"),
            serde_json::json!({ "plugin": "demo-plugin" })
        );
    }
}
