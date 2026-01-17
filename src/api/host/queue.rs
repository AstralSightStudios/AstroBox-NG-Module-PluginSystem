use crate::bindings::astrobox::psys_host;
use serde_json::json;

use super::{
    HostString, PluginCtx,
    permission::check_permission_declared_blocking,
};

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
        if !check_permission_declared_blocking(
            &app_handle,
            permissions.as_ref(),
            "queue",
            params,
        ) {
            return Ok(());
        }
        Ok(())
    }
}
