//! Level 4 的下载队列接口。

use frontbridge::invoke_frontend;
use serde_json::json;
use wasmtime_v4 as wasmtime;

use crate::api::host::permission::check_permission_declared_blocking;
use crate::v4::bindings::astrobox::psys_host_v4::queue;
use crate::v4::ctx::PluginCtxV4;

const FRONT_FILE_ADD_TO_QUEUE_METHOD: &str = "host/file/add_to_queue";

impl queue::Host for PluginCtxV4 {
    fn add_resource_to_queue(
        &mut self,
        res_type: queue::ResourceType,
        file_path: String,
    ) -> wasmtime::Result<()> {
        let plugin_name = self.plugin_name().to_string();
        let app_handle = self.app_handle();
        let permissions = self.permissions();
        let res_label = match res_type {
            queue::ResourceType::Quickapp => "quickapp",
            queue::ResourceType::Watchface => "watchface",
            queue::ResourceType::Firmware => "firmware",
        };
        let params = json!({
            "plugin": plugin_name,
            "resourceType": res_label,
            "filePath": file_path,
        });
        if !check_permission_declared_blocking(&app_handle, permissions.as_ref(), "queue", params) {
            return Ok(());
        }
        let payload = json!({ "files": [file_path] });
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
