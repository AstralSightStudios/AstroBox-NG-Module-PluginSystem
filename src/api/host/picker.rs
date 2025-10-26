use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, HostVec, PluginCtx};

impl psys_host::picker::Host for PluginCtx {}

impl psys_host::picker::HostWithStore for PluginCtx {
    fn pick_file<T>(
        accessor: &Accessor<T, Self>,
        config: psys_host::picker::PickConfig,
        filter: psys_host::picker::FilterConfig,
    ) -> impl core::future::Future<Output = FutureReader<psys_host::picker::PickResult>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = config;
                let _ = filter;
                Ok::<psys_host::picker::PickResult, Error>(psys_host::picker::PickResult {
                    name: HostString::default(),
                    data: HostVec::new(),
                })
            })
        });
        async move { future }
    }
}
