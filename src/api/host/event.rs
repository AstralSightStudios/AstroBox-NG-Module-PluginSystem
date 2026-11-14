use crate::bindings::astrobox::psys_host;

use super::{HostString, PluginCtx};

/// 宿主自己发给插件的插件消息（队列任务回报、传输进度、WebSocket 推送……）。
///
/// 比插件之间广播的消息多一个 `"host": true`。广播的外层 JSON 由宿主拼，插件只能决定
/// eventName 和 payload，造不出这个字段，所以插件可以靠它分辨消息是不是真的来自宿主，
/// 不会被别的插件用同名事件冒充。
pub(crate) fn host_plugin_message(event_name: &str, payload: &str) -> String {
    serde_json::json!({
        "eventName": event_name,
        "payload": payload,
        "host": true,
    })
    .to_string()
}

/// 把插件发出的事件广播给其它已加载插件。
///
/// 与 API Level 无关：投递走 `PluginRuntime::dispatch_plugin_message`，由它按目标
/// 插件自己的 Level 选择实例，因此 Level 2/3/4 都复用这一份。
pub(crate) fn broadcast_plugin_event(
    source_plugin: String,
    source_generation: u64,
    event_name: String,
    payload_raw: String,
) {
    let message = serde_json::json!({
        "eventName": event_name.clone(),
        "payload": payload_raw,
    })
    .to_string();

    tauri::async_runtime::spawn(async move {
        let log_name = event_name.clone();
        if let Err(err) = crate::with_plugin_manager_async({
            let payload = message.clone();
            let source_plugin = source_plugin.clone();
            move |pm| {
                // 源插件自身若已被卸载/换代，这次广播就作废，避免旧运行时的残留事件
                // 打到新实例上。
                let source_is_current = pm.plugins.get(&source_plugin).is_some_and(|plugin| {
                    plugin.state.loaded
                        && !plugin.state.disabled
                        && plugin.runtime.is_generation_current(source_generation)
                });
                let active_plugins = if source_is_current {
                    pm.plugins
                        .iter()
                        .filter(|(_, plugin)| plugin.state.loaded && !plugin.state.disabled)
                        .filter(|(name, _)| name.as_str() != source_plugin.as_str())
                        .map(|(name, plugin)| (name.clone(), plugin.runtime.clone()))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let event_name = event_name.clone();
                let payload = payload.clone();
                Box::pin(async move {
                    for (name, runtime) in active_plugins {
                        if let Err(err) = runtime.dispatch_plugin_message(payload.clone()).await {
                            log::error!(
                                "Failed to deliver plugin event '{}' to {}: {err}",
                                event_name.as_str(),
                                name
                            );
                        }
                    }
                })
            }
        })
        .await
        {
            log::error!(
                "Failed to broadcast plugin event '{}': {err}",
                log_name.as_str()
            );
        }
    });
}

impl psys_host::event::Host for PluginCtx {
    fn send_event(&mut self, event_name: HostString, payload: HostString) -> wasmtime::Result<()> {
        broadcast_plugin_event(
            self.plugin_name().to_string(),
            self.runtime_generation(),
            event_name.to_string(),
            payload.to_string(),
        );
        Ok(())
    }
}
