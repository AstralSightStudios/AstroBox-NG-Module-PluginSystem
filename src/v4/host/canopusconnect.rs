//! Level 4 的 CanopusConnect 接口：与手环上的 Canopus 模块互发文本/二进制消息。
//!
//! 线格式、设备端监听状态和重连补订都在 core 里（`corelib::device::canopus::connect`），
//! 这里只做权限、插件归属和类型转换。

use corelib::device::canopus::connect::{self as core, ConnectPayload};
use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::{check_permission_declared, resolve_device_name};
use crate::plugin::CanopusListenRegistration;
use crate::v4::bindings::astrobox::psys_host_v4::canopusconnect;
use crate::v4::ctx::PluginCtxV4;

/// 向模块发消息所需的权限。
const SEND_PERMISSION: &str = "canopusconnect.send";
/// 监听模块消息所需的权限。
const LISTEN_PERMISSION: &str = "canopusconnect.listen";

impl canopusconnect::Host for PluginCtxV4 {}

impl canopusconnect::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn send(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        module: String,
        payload: canopusconnect::Payload,
    ) -> wasmtime::Result<Result<(), String>> {
        if authorize(accessor, SEND_PERMISSION, &device_addr, &module)
            .await
            .is_none()
        {
            return Ok(Err("permission denied".into()));
        }
        Ok(core::send(&device_addr, &module, to_core(payload)).await)
    }

    async fn listen(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        module: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let Some(register_state) = authorize(accessor, LISTEN_PERMISSION, &device_addr, &module).await
        else {
            return Ok(Err("permission denied".into()));
        };
        if !register_state.has_canopus_export() {
            return Ok(Err(
                "plugin does not export astrobox:psys-plugin-v4/canopusconnect".into(),
            ));
        }
        let registration = CanopusListenRegistration {
            addr: device_addr,
            module,
        };
        // 同一插件重复监听不再计数；只有新登记的才去告诉设备。
        if !register_state.add_canopus_listen(registration.clone()).await {
            return Ok(Ok(()));
        }
        let result = core::subscribe(&registration.addr, &registration.module).await;
        if result.is_err() {
            register_state
                .remove_canopus_listen(&registration.addr, &registration.module)
                .await;
        }
        Ok(result)
    }

    async fn unlisten(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        module: String,
    ) -> wasmtime::Result<Result<(), String>> {
        // 取消监听不需要再问一次用户：放弃能力永远允许。
        let register_state = accessor.with(|mut access| access.get().register_state());
        if !register_state
            .remove_canopus_listen(&device_addr, &module)
            .await
        {
            return Ok(Ok(()));
        }
        Ok(core::unsubscribe(&device_addr, &module).await)
    }
}

/// 检查清单声明和用户授权，通过则返回该插件的登记表。
async fn authorize(
    accessor: &Accessor<PluginCtxV4, PluginCtxV4>,
    permission: &str,
    addr: &str,
    module: &str,
) -> Option<std::sync::Arc<crate::plugin::PluginRegisterState>> {
    let (app, plugin, permissions, register_state) = accessor.with(|mut access| {
        let ctx = access.get();
        (
            ctx.app_handle(),
            ctx.plugin_name().to_string(),
            ctx.permissions(),
            ctx.register_state(),
        )
    });
    let device_name = resolve_device_name(addr).await;
    check_permission_declared(
        &app,
        &permissions,
        permission,
        json!({
            "plugin": plugin,
            "addr": addr,
            "deviceName": device_name,
            "module": module,
        }),
    )
    .await
    .then_some(register_state)
}

fn to_core(payload: canopusconnect::Payload) -> ConnectPayload {
    match payload {
        canopusconnect::Payload::Text(text) => ConnectPayload::Text(text),
        canopusconnect::Payload::Binary(data) => ConnectPayload::Binary(data),
    }
}

pub(crate) fn from_core(payload: ConnectPayload) -> canopusconnect::Payload {
    match payload {
        ConnectPayload::Text(text) => canopusconnect::Payload::Text(text),
        ConnectPayload::Binary(data) => canopusconnect::Payload::Binary(data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_conversion_keeps_text_and_binary_apart() {
        let text = ConnectPayload::Text("你好".into());
        assert_eq!(to_core(from_core(text.clone())), text);
        let binary = ConnectPayload::Binary(vec![0, 1, 255]);
        assert_eq!(to_core(from_core(binary.clone())), binary);
    }
}
