use crate::bindings::astrobox::psys_host;

use super::{HostString, PluginCtx};

impl psys_host::event::Host for PluginCtx {
    fn send_event(&mut self, _event_name: HostString, _payload: HostString) {}
}
