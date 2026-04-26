use crate::bindings::astrobox::psys_host;
use crate::provider_action_bridge;

use super::{HostString, PluginCtx};

impl psys_host::provider_callback::Host for PluginCtx {
    fn resolve_provider_action(
        &mut self,
        request_id: HostString,
        response: HostString,
    ) -> wasmtime::Result<bool> {
        let request_id = request_id.to_string();
        let response = response.to_string();
        let response_len = response.len();
        let plugin_name = self.plugin_name().to_string();
        let resolved =
            provider_action_bridge::resolve_pending_provider_action(&request_id, response);

        if resolved {
            log::info!(
                target: "pluginsystem::provider_action",
                "[provider-action] callback resolved plugin={}, request_id={}, response_len={}",
                plugin_name,
                request_id,
                response_len,
            );
        } else {
            log::warn!(
                target: "pluginsystem::provider_action",
                "[provider-action] callback dropped plugin={}, request_id={}, response_len={}",
                plugin_name,
                request_id,
                response_len,
            );
        }

        Ok(resolved)
    }

    fn report_provider_action_progress(
        &mut self,
        request_id: HostString,
        progress: f32,
        status: HostString,
    ) -> wasmtime::Result<bool> {
        let request_id = request_id.to_string();
        let status = status.to_string();
        let plugin_name = self.plugin_name().to_string();
        let accepted = provider_action_bridge::report_pending_provider_action_progress(
            &request_id,
            progress,
            status.clone(),
        );

        if accepted {
            log::debug!(
                target: "pluginsystem::provider_action",
                "[provider-action] progress plugin={}, request_id={}, progress={}, status={}",
                plugin_name,
                request_id,
                progress,
                status,
            );
        } else {
            log::warn!(
                target: "pluginsystem::provider_action",
                "[provider-action] progress dropped plugin={}, request_id={}, progress={}, status={}",
                plugin_name,
                request_id,
                progress,
                status,
            );
        }

        Ok(accepted)
    }
}
