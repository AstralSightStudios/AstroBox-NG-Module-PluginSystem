use crate::bindings::astrobox::psys_host;

use super::{HostString, PluginCtx};

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        _res_type: psys_host::queue::ResourceType,
        _file_path: HostString,
    ) -> wasmtime::Result<()> {
        Ok(())
    }
}
