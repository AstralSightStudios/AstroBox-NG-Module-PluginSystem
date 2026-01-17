use anyhow::Error;
use corelib::device::xiaomi::{
    XiaomiDevice,
    components::resource::ResourceComponent,
};
use frontbridge::invoke_frontend;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

const FRONT_PERMISSION_METHOD: &str = "host/register/request_permission";

#[derive(Serialize)]
struct PermissionRequestPayload {
    operation: String,
    params: Value,
}

#[derive(Deserialize)]
struct PermissionResponsePayload {
    granted: bool,
}

async fn request_permission(
    app_handle: &AppHandle,
    operation: impl Into<String>,
    params: Value,
) -> Result<bool, Error> {
    let operation = operation.into();
    let payload = PermissionRequestPayload {
        operation: operation.clone(),
        params,
    };
    let resp: PermissionResponsePayload =
        invoke_frontend(app_handle, FRONT_PERMISSION_METHOD, payload).await?;
    Ok(resp.granted)
}

fn normalize_permission_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn extract_plugin_name(params: &Value) -> Option<String> {
    params
        .get("plugin")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| {
            params
                .get("plugin_name")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .or_else(|| {
            params
                .get("pluginName")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
}

pub(crate) fn is_permission_declared(permissions: &[String], required: &str) -> bool {
    let required = normalize_permission_name(required);
    if required.is_empty() {
        return false;
    }
    permissions.iter().any(|perm| perm == &required)
}

pub(crate) async fn check_permission(
    app_handle: &AppHandle,
    operation: impl Into<String>,
    params: Value,
) -> bool {
    let operation = operation.into();
    match request_permission(app_handle, operation.clone(), params).await {
        Ok(granted) => granted,
        Err(err) => {
            log::warn!(
                "[pluginsystem] permission request '{}' failed: {err}",
                operation
            );
            false
        }
    }
}

pub(crate) async fn check_permission_declared(
    app_handle: &AppHandle,
    permissions: &[String],
    operation: impl Into<String>,
    params: Value,
) -> bool {
    let operation = operation.into();
    let operation_label = operation.clone();
    let plugin = extract_plugin_name(&params).unwrap_or_else(|| "unknown".to_string());
    log::info!(
        "[pluginsystem] permission request start '{}' from {}",
        operation_label,
        plugin
    );
    if !is_permission_declared(permissions, &operation) {
        log::warn!(
            "[pluginsystem] permission '{}' not declared by plugin",
            operation_label
        );
        return false;
    }
    let granted = check_permission(app_handle, operation, params).await;
    log::info!(
        "[pluginsystem] permission request done '{}' from {} -> {}",
        operation_label,
        plugin,
        granted
    );
    granted
}

pub(crate) fn check_permission_declared_blocking(
    app_handle: &AppHandle,
    permissions: &[String],
    operation: impl Into<String>,
    params: Value,
) -> bool {
    tauri::async_runtime::block_on(check_permission_declared(
        app_handle,
        permissions,
        operation,
        params,
    ))
}

pub(crate) async fn resolve_device_name(addr: &str) -> Option<String> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    let addr = addr.to_string();
    corelib::ecs::with_rt_mut(move |rt| {
        rt.component_ref::<XiaomiDevice>(addr.as_str())
            .map(|device| device.name().to_string())
    })
    .await
}

pub(crate) async fn resolve_quick_app_name(
    device_addr: &str,
    pkg_name: &str,
) -> Option<String> {
    let device_addr = device_addr.trim();
    let pkg_name = pkg_name.trim();
    if device_addr.is_empty() || pkg_name.is_empty() {
        return None;
    }
    let device_addr = device_addr.to_string();
    let pkg_name = pkg_name.to_string();
    corelib::ecs::with_rt_mut(move |rt| {
        let entity = rt.device_entity(&device_addr)?;
        let resource_comp = rt.world().get::<ResourceComponent>(entity)?;
        resource_comp
            .quick_apps
            .iter()
            .find(|item| item.package_name == pkg_name)
            .and_then(|item| {
                let name = item.app_name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
    })
    .await
}
