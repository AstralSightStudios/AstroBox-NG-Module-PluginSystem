//! Level 4 的插件间事件广播接口。

use wasmtime_v4 as wasmtime;

use crate::v4::bindings::astrobox::psys_host_v4::event;
use crate::v4::ctx::PluginCtxV4;

impl event::Host for PluginCtxV4 {
    fn send_event(&mut self, event_name: String, payload: String) -> wasmtime::Result<()> {
        crate::api::host::event::broadcast_plugin_event(
            self.plugin_name().to_string(),
            self.runtime_generation(),
            event_name,
            payload,
        );
        Ok(())
    }
}
