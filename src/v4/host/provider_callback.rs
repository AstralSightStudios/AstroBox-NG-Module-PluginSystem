//! Level 4 的 provider 回调接口。纯本地操作，直接复用 provider_action_bridge。

use wasmtime_v4 as wasmtime;

use crate::provider_action_bridge;
use crate::v4::bindings::astrobox::psys_host_v4::provider_callback;
use crate::v4::ctx::PluginCtxV4;

impl provider_callback::Host for PluginCtxV4 {
    fn resolve_provider_action(
        &mut self,
        request_id: String,
        response: String,
    ) -> wasmtime::Result<bool> {
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
        request_id: String,
        progress: f32,
        status: String,
    ) -> wasmtime::Result<bool> {
        let plugin_name = self.plugin_name().to_string();
        let accepted = provider_action_bridge::report_pending_provider_action_progress(
            &request_id,
            progress,
            status.clone(),
        );

        if !accepted {
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
