use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, HostVec, PluginCtx};

impl psys_host::thirdpartyapp::Host for PluginCtx {}

impl psys_host::thirdpartyapp::HostWithStore for PluginCtx {
    fn launch_qa<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        app_info: psys_host::thirdpartyapp::AppInfo,
        page_name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                let _ = app_info;
                let _ = page_name;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn get_thirdparty_app_list<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
    ) -> impl core::future::Future<
        Output = FutureReader<core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>>,
    > + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = addr;
                Ok::<
                    core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                    Error,
                >(Ok(HostVec::new()))
            })
        });
        async move { future }
    }
}
