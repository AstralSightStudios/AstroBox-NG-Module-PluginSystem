use crate::bindings::astrobox::psys_host;
use crate::plugin::{
    CardRegistration, InterconnectRecvRegistration, ProviderRegistration, TransportRecvRegistration,
};
use anyhow::Error;
use frontbridge::invoke_frontend;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

const FRONT_PERMISSION_METHOD: &str = "host/register/request_permission";

#[derive(Serialize)]
struct PermissionRequestPayload {
    operation: String,
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct PermissionResponsePayload {
    granted: bool,
}

async fn request_permission(
    app_handle: &AppHandle,
    operation: impl Into<String>,
    params: serde_json::Value,
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

impl psys_host::register::Host for PluginCtx {}

impl psys_host::register::HostWithStore for PluginCtx {
    fn register_transport_recv<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        filter: psys_host::register::TransportRecvFiler,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = addr.to_string();
                let psys_host::register::TransportRecvFiler {
                    xiaomi_vela_v5_channel_id,
                    xiaomi_vela_v5_protobuf_typeid,
                } = filter;
                let params = json!({
                    "plugin": plugin_name,
                    "addr": addr.clone(),
                    "filter": {
                        "xiaomiVelaV5ChannelId": xiaomi_vela_v5_channel_id,
                        "xiaomiVelaV5ProtobufTypeid": xiaomi_vela_v5_protobuf_typeid,
                    }
                });

                if !request_permission(&app_handle, "register_transport_recv", params).await? {
                    return Ok(Err(()));
                }

                register_state
                    .register_transport_recv(TransportRecvRegistration {
                        addr,
                        filter: psys_host::register::TransportRecvFiler {
                            xiaomi_vela_v5_channel_id,
                            xiaomi_vela_v5_protobuf_typeid,
                        },
                    })
                    .await;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_interconnect_recv<T>(
        accessor: &Accessor<T, Self>,
        addr: HostString,
        pkg_name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let addr = addr.to_string();
                let pkg_name = pkg_name.to_string();
                let params = json!({
                    "plugin": plugin_name,
                    "addr": addr.clone(),
                    "pkgName": pkg_name.clone(),
                });

                if !request_permission(&app_handle, "register_interconnect_recv", params).await? {
                    return Ok(Err(()));
                }

                register_state
                    .register_interconnect_recv(InterconnectRecvRegistration { addr, pkg_name })
                    .await;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_deeplink_action<T>(
        accessor: &Accessor<T, Self>,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let params = json!({
                    "plugin": plugin_name,
                    "action": "deeplink",
                });

                if !request_permission(&app_handle, "register_deeplink_action", params).await? {
                    return Ok(Err(()));
                }

                if register_state.try_register_deeplink().await {
                    Ok::<core::result::Result<(), ()>, Error>(Ok(()))
                } else {
                    Ok::<core::result::Result<(), ()>, Error>(Err(()))
                }
            })
        });
        async move { future }
    }

    fn register_provider<T>(
        accessor: &Accessor<T, Self>,
        name: HostString,
        provider_type: psys_host::register::ProviderType,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let name = name.to_string();
                let provider_label = match &provider_type {
                    psys_host::register::ProviderType::Url => "url",
                    psys_host::register::ProviderType::Custom => "custom",
                };
                let params = json!({
                    "plugin": plugin_name,
                    "name": name.clone(),
                    "providerType": provider_label,
                });

                if !request_permission(&app_handle, "register_provider", params).await? {
                    return Ok(Err(()));
                }

                register_state
                    .register_provider(ProviderRegistration {
                        name,
                        provider_type,
                    })
                    .await;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }

    fn register_card<T>(
        accessor: &Accessor<T, Self>,
        card_type: psys_host::register::CardType,
        id: HostString,
        name: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let id = id.to_string();
                let name = name.to_string();
                let card_label = match &card_type {
                    psys_host::register::CardType::Element => "element",
                    psys_host::register::CardType::Text => "text",
                };
                let params = json!({
                    "plugin": plugin_name,
                    "type": card_label,
                    "id": id.clone(),
                    "name": name.clone(),
                });

                if !request_permission(&app_handle, "register_card", params).await? {
                    return Ok(Err(()));
                }

                register_state
                    .register_card(CardRegistration {
                        card_type,
                        id,
                        name,
                    })
                    .await;
                Ok::<core::result::Result<(), ()>, Error>(Ok(()))
            })
        });
        async move { future }
    }
}
