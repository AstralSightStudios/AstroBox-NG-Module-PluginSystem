use crate::bindings::astrobox::psys_host;
use anyhow::{Error, anyhow};
use corelib::device::xiaomi::components::{
    resource::{ResourceComponent, ResourceSystem},
    thirdparty_app::{AppInfo, ThirdpartyAppSystem},
};
use log::error;
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
                let addr = addr.to_string();
                let page_name = page_name.to_string();

                match launch_qa_impl(addr, app_info, page_name).await {
                    Ok(()) => Ok::<core::result::Result<(), ()>, Error>(Ok(())),
                    Err(err) => {
                        error!("Failed to launch third-party app: {err:?}");
                        Ok::<core::result::Result<(), ()>, Error>(Err(()))
                    }
                }
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
                let addr = addr.to_string();
                match get_thirdparty_app_list_impl(addr).await {
                    Ok(list) => Ok::<
                        core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                        Error,
                    >(Ok(list)),
                    Err(err) => {
                        error!("Failed to fetch third-party app list: {err:?}");
                        Ok::<
                            core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                            Error,
                        >(Err(()))
                    }
                }
            })
        });
        async move { future }
    }
}

async fn launch_qa_impl(
    device_addr: String,
    app_info: psys_host::thirdpartyapp::AppInfo,
    page_name: String,
) -> Result<(), Error> {
    let app_info = normalize_app_info(&device_addr, app_info).await?;
    corelib::ecs::with_rt_mut(move |rt| {
        rt.with_device_mut(&device_addr, |world, entity| {
            let mut system = world
                .get_mut::<ThirdpartyAppSystem>(entity)
                .ok_or_else(|| anyhow!("Thirdparty app system not found on {}", device_addr))?;
            system.launch_app(&app_info, &page_name);
            Ok(())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?
    })
    .await
}

async fn get_thirdparty_app_list_impl(
    device_addr: String,
) -> Result<HostVec<psys_host::thirdpartyapp::AppInfo>, Error> {
    let rx = corelib::ecs::with_rt_mut(move |rt| -> Result<_, Error> {
        rt.with_device_mut(&device_addr, |world, entity| {
            let mut system = world
                .get_mut::<ResourceSystem>(entity)
                .ok_or_else(|| anyhow!("Resource system not found on {}", device_addr))?;
            Ok::<_, Error>(system.request_quick_app_list())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?
    })
    .await?;

    let list = rx
        .await
        .map_err(|err| anyhow!("Quick app list response not received: {err:?}"))??;

    let mut host_list: HostVec<psys_host::thirdpartyapp::AppInfo> = HostVec::new();
    for item in list {
        host_list.push(psys_host::thirdpartyapp::AppInfo {
            package_name: item.package_name,
            fingerprint: fingerprint_to_host(item.fingerprint),
            version_code: item.version_code,
            can_remove: item.can_remove,
            app_name: item.app_name,
        });
    }

    Ok(host_list)
}

async fn normalize_app_info(
    device_addr: &str,
    app_info: psys_host::thirdpartyapp::AppInfo,
) -> Result<AppInfo, Error> {
    let package_name: String = app_info.package_name.into();
    if package_name.is_empty() {
        return Err(anyhow!("Third-party app package name is empty"));
    }

    let fingerprint = fingerprint_to_bytes(app_info.fingerprint)?;
    if !fingerprint.is_empty() {
        return Ok(AppInfo {
            package_name,
            fingerprint,
        });
    }

    resolve_app_info_from_component(device_addr, &package_name).await
}

async fn resolve_app_info_from_component(
    device_addr: &str,
    package_name: &str,
) -> Result<AppInfo, Error> {
    let device_addr = device_addr.to_string();
    let package_name = package_name.to_string();
    corelib::ecs::with_rt_mut(move |rt| {
        let entity = rt
            .device_entity(&device_addr)
            .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?;
        let resource_comp = rt
            .world()
            .get::<ResourceComponent>(entity)
            .ok_or_else(|| anyhow!("Resource component unavailable: {}", device_addr))?;

        resource_comp
            .quick_apps
            .iter()
            .find(|item| item.package_name == package_name)
            .map(|item| AppInfo {
                package_name: item.package_name.clone(),
                fingerprint: item.fingerprint.clone(),
            })
            .ok_or_else(|| anyhow!("Quick app {} not found on {}", package_name, device_addr))
    })
    .await
}

fn fingerprint_to_bytes(fingerprint: HostVec<u32>) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(fingerprint.len());
    for (idx, value) in fingerprint.into_iter().enumerate() {
        let byte =
            u8::try_from(value).map_err(|_| anyhow!("fingerprint[{idx}] out of range: {value}"))?;
        out.push(byte);
    }
    Ok(out)
}

fn fingerprint_to_host(fingerprint: Vec<u8>) -> HostVec<u32> {
    let mut out: HostVec<u32> = HostVec::new();
    out.reserve(fingerprint.len());
    for value in fingerprint {
        out.push(value as u32);
    }
    out
}
