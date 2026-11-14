use crate::bindings::astrobox::psys_host;
use anyhow::{Error, anyhow};
use corelib::device::xiaomi::components::{
    resource::{ResourceComponent, ResourceSystem},
    thirdparty_app::{AppInfo, ThirdpartyAppSystem},
};
use log::error;
use serde_json::json;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, HostVec, PluginCtx, permission::check_permission_declared};

/// 向设备要快应用列表的等待上限。以前没有上限：手环刚重启、应用服务还没起来时这条请求会石沉大海，
/// 调用方一直挂到设备实体被替换（手动断开重连）才以 RecvError 结束。
const QUICK_APP_LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);
/// 按包名找快应用、缓存里没有时现取一次列表的等待上限（发互联消息、启动快应用的路径上）。
const QUICK_APP_LOOKUP_REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

impl psys_host::thirdpartyapp::Host for PluginCtx {}

impl psys_host::thirdpartyapp::HostWithStore for PluginCtx {
    fn launch_qa<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        app_info: psys_host::thirdpartyapp::AppInfo,
        page_name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = addr.to_string();
                let page_name = page_name.to_string();
                let package_name = app_info.package_name.clone().to_string();

                let params = json!({
                    "plugin": plugin_name,
                    "addr": addr.clone(),
                    "pkgName": package_name,
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "thirdpartyapp",
                    params,
                )
                .await
                {
                    return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                }

                let fingerprint = match fingerprint_to_bytes(app_info.fingerprint) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        error!("Invalid third-party app fingerprint: {err:?}");
                        return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                    }
                };
                match launch_qa_raw(
                    addr,
                    app_info.package_name.to_string(),
                    fingerprint,
                    page_name,
                )
                .await
                {
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
                    "thirdpartyapp",
                    params,
                )
                .await
                {
                    return Ok::<
                        core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                        Error,
                    >(Err(()));
                }
                match get_thirdparty_app_list_raw(addr).await {
                    Ok(list) => {
                        let mut host_list: HostVec<psys_host::thirdpartyapp::AppInfo> =
                            HostVec::new();
                        for item in list {
                            host_list.push(psys_host::thirdpartyapp::AppInfo {
                                package_name: item.package_name,
                                fingerprint: fingerprint_to_host(item.fingerprint),
                                version_code: item.version_code,
                                can_remove: item.can_remove,
                                app_name: item.app_name,
                            });
                        }
                        Ok::<
                            core::result::Result<HostVec<psys_host::thirdpartyapp::AppInfo>, ()>,
                            Error,
                        >(Ok(host_list))
                    }
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

/// 启动快应用。指纹传空则按包名从设备组件里补齐。
pub(crate) async fn launch_qa_raw(
    device_addr: String,
    package_name: String,
    fingerprint: Vec<u8>,
    page_name: String,
) -> Result<(), Error> {
    let app_info = normalize_app_info(&device_addr, package_name, fingerprint).await?;
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

/// 取快应用列表，返回 corelib 原生条目。
///
/// 返回中性类型而不是某个 API Level 的 bindgen 类型，Level 2/3 与 Level 4 各自
/// 在调用点转换成自己世界里的 `app-info`，逻辑只维护这一份。
pub(crate) async fn get_thirdparty_app_list_raw(
    device_addr: String,
) -> Result<Vec<pb::xiaomi::protocol::AppItem>, Error> {
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

    await_quick_app_list(rx, QUICK_APP_LIST_TIMEOUT).await
}

async fn await_quick_app_list(
    rx: tokio::sync::oneshot::Receiver<anyhow::Result<Vec<pb::xiaomi::protocol::AppItem>>>,
    timeout: std::time::Duration,
) -> Result<Vec<pb::xiaomi::protocol::AppItem>, Error> {
    // 超时丢掉接收端即可：RequestSlot 会把已关闭的等待方剔掉，下一次请求会重新向设备要。
    tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_| anyhow!("Quick app list response timed out after {}s", timeout.as_secs()))?
        .map_err(|err| anyhow!("Quick app list response not received: {err:?}"))?
}

/// 按包名找设备上的快应用，拿到发消息、启动所需的指纹。
///
/// 先查设备组件里缓存的列表；没有就现向设备要一次再查。缓存只在有人取过列表时才填：
/// 每次（重新）连接都是新的设备实体、缓存是空的，手环重启后自动回连时如果第一次取列表恰好
/// 没回（应用服务还没起来），插件之后发的每条互联消息都会报「Quick app … not found」，
/// 直到用户手动断开重连。同时并发的几次查找共用同一条列表请求（RequestSlot 去重）。
pub(crate) async fn resolve_quick_app(
    device_addr: &str,
    package_name: &str,
) -> Result<AppInfo, Error> {
    if let Some(info) = find_cached_quick_app(device_addr, package_name).await? {
        return Ok(info);
    }

    let addr = device_addr.to_string();
    let rx = corelib::ecs::with_rt_mut(move |rt| -> Result<_, Error> {
        rt.with_device_mut(&addr, |world, entity| {
            let mut system = world
                .get_mut::<ResourceSystem>(entity)
                .ok_or_else(|| anyhow!("Resource system not found on {}", addr))?;
            Ok::<_, Error>(system.request_quick_app_list())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", addr))?
    })
    .await?;
    let list = await_quick_app_list(rx, QUICK_APP_LOOKUP_REFRESH_TIMEOUT)
        .await
        .map_err(|err| {
            anyhow!(
                "Quick app {} not found on {} (refreshing the app list failed: {err})",
                package_name,
                device_addr
            )
        })?;
    list.iter()
        .find(|item| item.package_name == package_name)
        .map(|item| AppInfo {
            package_name: item.package_name.clone(),
            fingerprint: item.fingerprint.clone(),
        })
        .ok_or_else(|| anyhow!("Quick app {} not found on {}", package_name, device_addr))
}

async fn find_cached_quick_app(
    device_addr: &str,
    package_name: &str,
) -> Result<Option<AppInfo>, Error> {
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
        Ok(resource_comp
            .quick_apps
            .iter()
            .find(|item| item.package_name == package_name)
            .map(|item| AppInfo {
                package_name: item.package_name.clone(),
                fingerprint: item.fingerprint.clone(),
            }))
    })
    .await
}

async fn normalize_app_info(
    device_addr: &str,
    package_name: String,
    fingerprint: Vec<u8>,
) -> Result<AppInfo, Error> {
    if package_name.is_empty() {
        return Err(anyhow!("Third-party app package name is empty"));
    }

    if !fingerprint.is_empty() {
        return Ok(AppInfo {
            package_name,
            fingerprint,
        });
    }

    resolve_quick_app(device_addr, &package_name).await
}

pub(crate) fn fingerprint_to_bytes(fingerprint: HostVec<u32>) -> Result<Vec<u8>, Error> {
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
