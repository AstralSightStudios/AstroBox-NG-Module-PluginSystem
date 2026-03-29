use crate::bindings::astrobox::psys_host;
use anyhow::{Error, anyhow};
use corelib::device::xiaomi::components::{resource::ResourceSystem, watchface::WatchfaceSystem};
use log::error;
use serde_json::json;
use wasmtime::component::{Accessor, FutureReader};

use crate::api::host::{HostString, HostVec, PluginCtx, permission::check_permission_declared};

impl psys_host::watchface::Host for PluginCtx {}

impl psys_host::watchface::HostWithStore for PluginCtx {
    fn get_watchface_list<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
    ) -> impl core::future::Future<
        Output = FutureReader<
            core::result::Result<HostVec<psys_host::watchface::WatchfaceInfo>, ()>,
        >,
    > + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = addr.to_string();
                let params = json!({
                    "plugin": plugin_name,
                    "addr": addr.clone(),
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "watchface",
                    params,
                )
                .await
                {
                    return Ok::<
                        core::result::Result<HostVec<psys_host::watchface::WatchfaceInfo>, ()>,
                        Error,
                    >(Err(()));
                }

                match get_watchface_list_impl(addr).await {
                    Ok(list) => Ok::<
                        core::result::Result<HostVec<psys_host::watchface::WatchfaceInfo>, ()>,
                        Error,
                    >(Ok(list)),
                    Err(err) => {
                        error!("Failed to fetch watchface list: {err:?}");
                        Ok::<
                            core::result::Result<HostVec<psys_host::watchface::WatchfaceInfo>, ()>,
                            Error,
                        >(Err(()))
                    }
                }
            })
        });
        async move { future }
    }

    fn set_current_watchface<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        watchface_id: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = addr.to_string();
                let watchface_id = watchface_id.to_string();
                let params = json!({
                    "plugin": plugin_name,
                    "addr": addr.clone(),
                    "watchfaceId": watchface_id.clone(),
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "watchface",
                    params,
                )
                .await
                {
                    return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                }

                match set_current_watchface_impl(addr, watchface_id).await {
                    Ok(()) => Ok::<core::result::Result<(), ()>, Error>(Ok(())),
                    Err(err) => {
                        error!("Failed to set current watchface: {err:?}");
                        Ok::<core::result::Result<(), ()>, Error>(Err(()))
                    }
                }
            })
        });
        async move { future }
    }
}

async fn get_watchface_list_impl(
    device_addr: String,
) -> Result<HostVec<psys_host::watchface::WatchfaceInfo>, Error> {
    let device_addr_for_lookup = device_addr.clone();
    let rx = corelib::ecs::with_rt_mut(move |rt| -> Result<_, Error> {
        let addr_for_error = device_addr_for_lookup.clone();
        rt.with_device_mut(&device_addr_for_lookup, |world, entity| {
            let mut system = world
                .get_mut::<ResourceSystem>(entity)
                .ok_or_else(|| anyhow!("Resource system not found on {}", addr_for_error))?;
            Ok::<_, Error>(system.request_watchface_list())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", addr_for_error))?
    })
    .await?;

    let list = rx
        .await
        .map_err(|err| anyhow!("Watchface list response not received: {err:?}"))??;

    let mut host_list: HostVec<psys_host::watchface::WatchfaceInfo> = HostVec::new();
    for item in list {
        host_list.push(psys_host::watchface::WatchfaceInfo {
            id: item.id,
            name: item.name,
            is_current: item.is_current,
        });
    }
    Ok(host_list)
}

async fn set_current_watchface_impl(
    device_addr: String,
    watchface_id: String,
) -> Result<(), Error> {
    let watchface_id = watchface_id.trim().to_string();
    if watchface_id.is_empty() {
        return Err(anyhow!("watchface id is empty"));
    }

    let device_addr_for_lookup = device_addr.clone();
    corelib::ecs::with_rt_mut(move |rt| {
        let addr_for_error = device_addr_for_lookup.clone();
        rt.with_device_mut(&device_addr_for_lookup, |world, entity| {
            let mut system = world
                .get_mut::<WatchfaceSystem>(entity)
                .ok_or_else(|| anyhow!("Watchface system not found on {}", addr_for_error))?;
            system.set_watchface(&watchface_id);
            Ok(())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", addr_for_error))?
    })
    .await
}
