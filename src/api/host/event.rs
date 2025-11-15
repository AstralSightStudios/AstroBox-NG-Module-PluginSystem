use crate::bindings::astrobox::psys_host;

use super::{HostString, PluginCtx};

impl psys_host::event::Host for PluginCtx {
    fn send_event(&mut self, event_name: HostString, payload: HostString) -> wasmtime::Result<()> {
        let event_name = event_name.to_string();
        let payload_raw = payload.to_string();

        let message = serde_json::json!({
            "eventName": event_name.clone(),
            "payload": payload_raw,
        })
        .to_string();

        let dispatch_name = event_name.clone();
        let log_name = event_name;
        let dispatch_payload = message.clone();

        tauri::async_runtime::spawn({
            let dispatch_payload = dispatch_payload;
            let dispatch_name = dispatch_name;
            let log_name = log_name;
            async move {
                if let Err(err) = crate::with_plugin_manager_async({
                    let event_name = dispatch_name.clone();
                    let payload = dispatch_payload.clone();
                    move |pm| {
                        let active_plugins = pm
                            .plugins
                            .iter()
                            .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
                            .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
                            .collect::<Vec<_>>();
                        let event_name = event_name.clone();
                        let payload = payload.clone();
                        async move {
                            for (name, runtime) in active_plugins {
                                if let Err(err) =
                                    runtime.dispatch_plugin_message(payload.clone()).await
                                {
                                    log::error!(
                                        "Failed to deliver plugin event '{}' to {}: {err}",
                                        event_name.as_str(),
                                        name
                                    );
                                }
                            }
                        }
                    }
                })
                .await
                {
                    log::error!(
                        "Failed to broadcast plugin event '{}': {err}",
                        log_name.as_str()
                    );
                }
            }
        });
        Ok(())
    }
}
