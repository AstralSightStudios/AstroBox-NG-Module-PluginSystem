use crate::astrobox::psys_host;

struct PluginCtx;

impl psys_host::config::Host for PluginCtx {
    fn read(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<(
        wasmtime::component::__internal::String,
        wasmtime::component::__internal::String,
    )> {
        todo!()
    }

    fn write(
        &mut self,
        content: wasmtime::component::__internal::Vec<(
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )>,
    ) -> () {
        todo!()
    }
}

impl psys_host::debug::Host for PluginCtx {}

impl psys_host::device::Host for PluginCtx {
    fn get_device_list(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<psys_host::device::DeviceInfo> {
        todo!()
    }

    fn get_connected_device_list(
        &mut self,
    ) -> wasmtime::component::__internal::Vec<psys_host::device::DeviceInfo> {
        todo!()
    }
}

impl psys_host::event::Host for PluginCtx {
    fn send_event(
        &mut self,
        event_name: wasmtime::component::__internal::String,
        payload: wasmtime::component::__internal::String,
    ) -> () {
        todo!()
    }
}

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::queue::Host for PluginCtx {
    fn add_resource_to_queue(
        &mut self,
        res_type: psys_host::queue::ResourceType,
        file_path: wasmtime::component::__internal::String,
    ) -> () {
        todo!()
    }
}

impl psys_host::thirdpartyapp::Host for PluginCtx {}
