use crate::bindings::astrobox::psys_host;
use anyhow::{Context, Error};
use corelib::device::xiaomi::XiaomiDevice;
use frontbridge::invoke_frontend;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager;
use wasmtime::component::{Access, FutureReader};

use super::{HostString, HostVec, PluginCtx, permission::check_permission_declared};

const FRONT_DEVICE_LIST_METHOD: &str = "host/device/get_device_list";

#[derive(Debug, Deserialize)]
struct StoredDeviceRecord {
    name: Option<String>,
    addr: Option<String>,
}

impl StoredDeviceRecord {
    fn into_psys_device(self) -> Option<psys_host::device::DeviceInfo> {
        match (self.name, self.addr) {
            (Some(name), Some(addr)) if !name.is_empty() && !addr.is_empty() => {
                Some(psys_host::device::DeviceInfo { name, addr })
            }
            _ => None,
        }
    }
}

impl psys_host::device::Host for PluginCtx {}

impl psys_host::device::HostWithStore for PluginCtx {
    fn get_device_list<T>(
        mut store: Access<'_, T, Self>,
    ) -> FutureReader<HostVec<psys_host::device::DeviceInfo>> {
        let app_handle = store.get().app_handle();
        let plugin_name = store.get().plugin_name().to_string();
        let permissions = store.get().permissions();
        let future = {
            let app_handle = app_handle.clone();
            FutureReader::new(
                &mut store,
                crate::api::host::AnyhowFuture(async move {
                    log::info!("[plugin:{}] device list request (history)", plugin_name);
                    if !check_permission_declared(
                        &app_handle,
                        permissions.as_ref(),
                        "device",
                        json!({ "plugin": plugin_name.clone() }),
                    )
                    .await
                    {
                        return Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(HostVec::new());
                    }

                    let devices: Vec<StoredDeviceRecord> =
                        invoke_frontend(&app_handle, FRONT_DEVICE_LIST_METHOD, ())
                            .await
                            .context("invoke frontend get_device_list")?;

                    let mut ret: HostVec<psys_host::device::DeviceInfo> = HostVec::new();
                    devices
                        .into_iter()
                        .filter_map(StoredDeviceRecord::into_psys_device)
                        .for_each(|dev| ret.push(dev));

                    log::info!(
                        "[plugin:{}] device list return {} items",
                        plugin_name,
                        ret.len()
                    );
                    Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(ret)
                }),
            )
        };
        future.expect("failed to create host future reader")
    }

    fn get_connected_device_list<T>(
        mut store: Access<'_, T, Self>,
    ) -> FutureReader<HostVec<psys_host::device::DeviceInfo>> {
        let app_handle = store.get().app_handle();
        let plugin_name = store.get().plugin_name().to_string();
        let permissions = store.get().permissions();
        let future = {
            FutureReader::new(
                &mut store,
                crate::api::host::AnyhowFuture(async move {
                    log::info!("[plugin:{}] connected device list request", plugin_name);
                    if !check_permission_declared(
                        &app_handle,
                        permissions.as_ref(),
                        "device",
                        json!({ "plugin": plugin_name.clone() }),
                    )
                    .await
                    {
                        return Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(HostVec::new());
                    }

                    let ret = corelib::ecs::with_rt_mut(|rt| {
                        rt.device_ids()
                            .filter_map(|device_id| {
                                rt.component_ref::<XiaomiDevice>(device_id.as_str())
                                    .map(|device| psys_host::device::DeviceInfo {
                                        addr: device.addr().to_string(),
                                        name: device.name().to_string(),
                                    })
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                    log::info!(
                        "[plugin:{}] connected device list return {} items",
                        plugin_name,
                        ret.len()
                    );
                    Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(ret)
                }),
            )
        };
        future.expect("failed to create host future reader")
    }

    fn disconnect_device<T>(
        mut store: Access<'_, T, Self>,
        device_addr: HostString,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let app_handle = store.get().app_handle();
        let plugin_name = store.get().plugin_name().to_string();
        let permissions = store.get().permissions();
        let future = {
            FutureReader::new(
                &mut store,
                crate::api::host::AnyhowFuture(async move {
                    let addr = device_addr.to_string();

                    if !check_permission_declared(
                        &app_handle,
                        permissions.as_ref(),
                        "device",
                        json!({ "plugin": plugin_name.clone() }),
                    )
                    .await
                    {
                        return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                    }

                    let Some(window) = app_handle.clone().get_webview_window("main") else {
                        log::warn!(
                            "[plugin:{}] disconnect_device failed: main window not found",
                            plugin_name
                        );
                        return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                    };

                    let addr_json =
                        serde_json::to_string(addr.as_str()).unwrap_or_else(|_| "\"\"".to_string());
                    let script = format!(
                        "window.__TAURI_INTERNALS__.invoke('miwear_disconnect', {{ addr: {} }})",
                        addr_json
                    );

                    if let Err(err) = window.eval(script.as_str()) {
                        log::warn!(
                            "[plugin:{}] disconnect_device eval failed: {err}",
                            plugin_name
                        );
                        return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                    }

                    Ok::<core::result::Result<(), ()>, Error>(Ok(()))
                }),
            )
        };
        future.expect("failed to create host future reader")
    }
}
