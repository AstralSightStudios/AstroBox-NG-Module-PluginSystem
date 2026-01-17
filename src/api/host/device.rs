use crate::bindings::astrobox::psys_host;
use anyhow::{Context, Error};
use corelib::device::xiaomi::XiaomiDevice;
use frontbridge::invoke_frontend;
use serde_json::json;
use serde::Deserialize;
use tauri::Manager;
use wasmtime::component::{Accessor, FutureReader};

use super::{
    HostString, HostVec, PluginCtx,
    permission::check_permission_declared,
};

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
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostVec<psys_host::device::DeviceInfo>>> + Send
    {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            let app_handle = app_handle.clone();
            FutureReader::new(instance, &mut access, async move {
                log::info!(
                    "[pluginsystem] device list request (history) from {}",
                    plugin_name
                );
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "device",
                    json!({ "plugin": plugin_name }),
                )
                .await
                {
                    return Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(
                        HostVec::new(),
                    );
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
                    "[pluginsystem] device list return {} items",
                    ret.len()
                );
                Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(ret)
            })
        });
        async move { future }
    }

    fn get_connected_device_list<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<HostVec<psys_host::device::DeviceInfo>>> + Send
    {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                log::info!(
                    "[pluginsystem] connected device list request from {}",
                    plugin_name
                );
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "device",
                    json!({ "plugin": plugin_name }),
                )
                .await
                {
                    return Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(
                        HostVec::new(),
                    );
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
                    "[pluginsystem] connected device list return {} items",
                    ret.len()
                );
                Ok::<HostVec<psys_host::device::DeviceInfo>, Error>(ret)
            })
        });
        async move { future }
    }

    fn disconnect_device<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = device_addr;

                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "device",
                    json!({ "plugin": plugin_name }),
                )
                .await
                {
                    return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                }

                app_handle
                    .clone()
                    .get_webview_window("main")
                    .unwrap()
                    .eval(format!(
                        "window.__TAURI_INTERNALS__.invoke('miwear_disconnect', {{ addr: '{}' }})",
                        addr
                    ))
                    .unwrap();

                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }
}
