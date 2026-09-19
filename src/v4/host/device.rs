//! Level 4 的 device 接口。

use anyhow::Context;
use corelib::device::xiaomi::XiaomiDevice;
use frontbridge::invoke_frontend;
use serde::Deserialize;
use serde_json::json;
use tauri::Manager;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::device;
use crate::v4::ctx::PluginCtxV4;

const FRONT_DEVICE_LIST_METHOD: &str = "host/device/get_device_list";

#[derive(Debug, Deserialize)]
struct StoredDeviceRecord {
    name: Option<String>,
    addr: Option<String>,
}

impl StoredDeviceRecord {
    fn into_device_info(self) -> Option<device::DeviceInfo> {
        match (self.name, self.addr) {
            (Some(name), Some(addr)) if !name.is_empty() && !addr.is_empty() => {
                Some(device::DeviceInfo { name, addr })
            }
            _ => None,
        }
    }
}

impl device::Host for PluginCtxV4 {}

impl device::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn get_device_list(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<Vec<device::DeviceInfo>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        log::info!("[plugin:{}] device list request (history)", plugin_name);
        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "device",
            json!({ "plugin": plugin_name.clone() }),
        )
        .await
        {
            return Ok(Vec::new());
        }

        let devices: Vec<StoredDeviceRecord> =
            invoke_frontend(&app_handle, FRONT_DEVICE_LIST_METHOD, ())
                .await
                .context("invoke frontend get_device_list")
                .map_err(wasmtime::Error::from_anyhow)?;

        let ret: Vec<device::DeviceInfo> = devices
            .into_iter()
            .filter_map(StoredDeviceRecord::into_device_info)
            .collect();
        log::info!(
            "[plugin:{}] device list return {} items",
            plugin_name,
            ret.len()
        );
        Ok(ret)
    }

    async fn get_connected_device_list(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<Vec<device::DeviceInfo>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "device",
            json!({ "plugin": plugin_name.clone() }),
        )
        .await
        {
            return Ok(Vec::new());
        }

        let ret = corelib::ecs::with_rt_mut(|rt| {
            rt.device_ids()
                .filter_map(|device_id| {
                    rt.component_ref::<XiaomiDevice>(device_id.as_str())
                        .map(|dev| device::DeviceInfo {
                            addr: dev.addr().to_string(),
                            name: dev.name().to_string(),
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
        Ok(ret)
    }

    async fn disconnect_device(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "device",
            json!({ "plugin": plugin_name.clone() }),
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        let Some(window) = app_handle.get_webview_window("main") else {
            log::warn!(
                "[plugin:{}] disconnect_device failed: main window not found",
                plugin_name
            );
            return Ok(Err("main window not found".to_string()));
        };

        let addr_json = serde_json::to_string(addr.as_str()).unwrap_or_else(|_| "\"\"".to_string());
        let script = format!(
            "window.__TAURI_INTERNALS__.invoke('miwear_disconnect', {{ addr: {} }})",
            addr_json
        );

        if let Err(err) = window.eval(script.as_str()) {
            log::warn!(
                "[plugin:{}] disconnect_device eval failed: {err}",
                plugin_name
            );
            return Ok(Err(err.to_string()));
        }

        Ok(Ok(()))
    }
}
