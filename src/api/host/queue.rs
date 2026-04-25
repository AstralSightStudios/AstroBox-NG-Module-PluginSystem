use crate::bindings::astrobox::psys_host;
use frontbridge::invoke_frontend;
use serde_json::json;

use super::{HostString, PluginCtx, permission::check_permission_declared_blocking};

const FRONT_FILE_ADD_TO_QUEUE_METHOD: &str = "host/file/add_to_queue";

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        res_type: psys_host::queue::ResourceType,
        file_path: HostString,
    ) -> wasmtime::Result<()> {
        let plugin_name = self.plugin_name().to_string();
        let app_handle = self.app_handle();
        let permissions = self.permissions();
        let res_label = match res_type {
            psys_host::queue::ResourceType::Quickapp => "quickapp",
            psys_host::queue::ResourceType::Watchface => "watchface",
            psys_host::queue::ResourceType::Firmware => "firmware",
        };
        let params = json!({
            "plugin": plugin_name,
            "resourceType": res_label,
            "filePath": file_path.to_string(),
        });
        if !check_permission_declared_blocking(&app_handle, permissions.as_ref(), "queue", params) {
            return Ok(());
        }
        let payload = json!({
            "files": [file_path.to_string()],
        });
        if let Err(err) = tauri::async_runtime::block_on(async {
            invoke_frontend::<bool, _>(&app_handle, FRONT_FILE_ADD_TO_QUEUE_METHOD, payload).await
        }) {
            log::warn!(
                "[plugin:{}] failed to add resource to frontend queue: {}",
                plugin_name,
                err
            );
        }
        Ok(())
    }
}
