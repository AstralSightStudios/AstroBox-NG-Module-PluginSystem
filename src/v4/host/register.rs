//! Level 4 的注册接口。
//!
//! 注册表本身是跨 API Level 共享的，里面存的是 Level 2/3 的 bindgen 类型，
//! 所以这里把 v4 的枚举转换过去，`PluginManager` 侧不需要区分插件版本。

use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::{
    check_permission_declared, resolve_device_name, resolve_quick_app_name,
};
use crate::bindings::astrobox::psys_host as psys_host_v2;
use crate::plugin::{
    CardRegistration, InterconnectRecvRegistration, ProviderRegistration, TransportRecvRegistration,
};
use crate::v4::bindings::astrobox::psys_host_v4::register;
use crate::v4::ctx::PluginCtxV4;

impl register::Host for PluginCtxV4 {}

impl register::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn register_transport_recv(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        filter: register::TransportRecvFiler,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions, register_state) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.register_state(),
            )
        });

        let register::TransportRecvFiler {
            xiaomi_vela_v5_channel_id,
            xiaomi_vela_v5_protobuf_typeid,
        } = filter;
        let device_name = resolve_device_name(&addr).await;
        let params = json!({
            "plugin": plugin_name,
            "addr": addr.clone(),
            "deviceName": device_name,
            "filter": {
                "xiaomiVelaV5ChannelId": xiaomi_vela_v5_channel_id,
                "xiaomiVelaV5ProtobufTypeid": xiaomi_vela_v5_protobuf_typeid,
            }
        });

        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "register_transport_recv",
            params,
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        register_state
            .register_transport_recv(TransportRecvRegistration {
                addr,
                filter: psys_host_v2::register::TransportRecvFiler {
                    xiaomi_vela_v5_channel_id,
                    xiaomi_vela_v5_protobuf_typeid,
                },
            })
            .await;
        Ok(Ok(()))
    }

    async fn register_interconnect_recv(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        pkg_name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions, register_state) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.register_state(),
            )
        });

        let app_name = resolve_quick_app_name(&addr, &pkg_name).await;
        let params = json!({
            "plugin": plugin_name,
            "addr": addr.clone(),
            "pkgName": pkg_name.clone(),
            "appName": app_name,
        });

        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "register_interconnect_recv",
            params,
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        register_state
            .register_interconnect_recv(InterconnectRecvRegistration { addr, pkg_name })
            .await;
        Ok(Ok(()))
    }

    async fn register_deeplink_action(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions, register_state) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.register_state(),
            )
        });

        let params = json!({ "plugin": plugin_name, "action": "deeplink" });
        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "register_deeplink_action",
            params,
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        if register_state.try_register_deeplink().await {
            Ok(Ok(()))
        } else {
            Ok(Err("deeplink already registered".to_string()))
        }
    }

    async fn register_provider(
        accessor: &Accessor<PluginCtxV4, Self>,
        name: String,
        provider_type: register::ProviderType,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions, register_state) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.register_state(),
            )
        });

        let provider_label = match provider_type {
            register::ProviderType::Url => "url",
            register::ProviderType::Custom => "custom",
        };
        let params = json!({
            "plugin": plugin_name,
            "name": name.clone(),
            "providerType": provider_label,
        });

        if !check_permission_declared(
            &app_handle,
            permissions.as_ref(),
            "register_provider",
            params,
        )
        .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        register_state
            .register_provider(ProviderRegistration {
                name,
                provider_type: match provider_type {
                    register::ProviderType::Url => psys_host_v2::register::ProviderType::Url,
                    register::ProviderType::Custom => psys_host_v2::register::ProviderType::Custom,
                },
            })
            .await;
        Ok(Ok(()))
    }

    async fn register_card(
        accessor: &Accessor<PluginCtxV4, Self>,
        card_type: register::CardType,
        id: String,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let register_state = accessor.with(|mut access| access.get().register_state());

        register_state
            .register_card(CardRegistration {
                card_type: match card_type {
                    register::CardType::Element => psys_host_v2::register::CardType::Element,
                    register::CardType::Text => psys_host_v2::register::CardType::Text,
                },
                id,
                name,
            })
            .await;
        Ok(Ok(()))
    }
}
