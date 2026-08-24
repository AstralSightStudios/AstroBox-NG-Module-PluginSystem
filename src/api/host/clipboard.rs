use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use serde_json::{Value, json};
use tauri_plugin_clipboard_manager::ClipboardExt;
use wasmtime::component::{Access, FutureReader};

use super::{HostString, PluginCtx, permission::check_permission_declared};

const READ_PERMISSION: &str = "clipboard.read";
const WRITE_PERMISSION: &str = "clipboard.write";

impl psys_host::clipboard::Host for PluginCtx {}

impl<T> psys_host::clipboard::HostWithStore<T> for PluginCtx {
    fn read_text(
        mut access: Access<'_, T, Self>,
    ) -> FutureReader<core::result::Result<HostString, ()>> {
        let app_handle = access.get().app_handle();
        let plugin_name = access.get().plugin_name().to_string();
        let permissions = access.get().permissions();
        crate::api::host::new_future_reader!(&mut access, async move {
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
    }

    fn write_text(
        mut access: Access<'_, T, Self>,
        text: HostString,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let app_handle = access.get().app_handle();
        let plugin_name = access.get().plugin_name().to_string();
        let permissions = access.get().permissions();
        let text = text.to_string();
        crate::api::host::new_future_reader!(&mut access, async move {
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
