//! Level 4 的 transport 接口。编解码与发包逻辑与 Level 2/3 共用。

use pb::xiaomi::protocol::WearPacket;
use prost::Message;
use serde_json::json;
use serde_json::to_string_pretty;
use corelib::device::xiaomi::packet::v2::layer2::L2Channel;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::permission::{check_permission_declared, resolve_device_name};
use crate::api::host::transport::{
    REQUEST_TIMEOUT, decode_pb_packet, send_xiaomi_pb_packet, transport_protocol_supported,
};
use crate::transport_runtime;
use crate::v4::bindings::astrobox::psys_host_v4::transport;
use crate::v4::ctx::PluginCtxV4;

impl transport::Host for PluginCtxV4 {
    fn to_json(
        &mut self,
        protocol: transport::Protocol,
        data: Vec<u8>,
    ) -> wasmtime::Result<String> {
        match protocol {
            transport::Protocol::XiaomiVelaV5Protobuf => {
                let packet = match WearPacket::decode(data.as_slice()) {
                    Ok(packet) => packet,
                    Err(err) => {
                        log::warn!("[pluginsystem] transport.to_json decode failed: {}", err);
                        return Ok(String::new());
                    }
                };
                Ok(to_string_pretty(&packet).unwrap_or_default())
            }
        }
    }

    fn from_json(
        &mut self,
        protocol: transport::Protocol,
        data: String,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        match protocol {
            transport::Protocol::XiaomiVelaV5Protobuf => {
                let packet: WearPacket = match serde_json::from_str(data.as_str()) {
                    Ok(packet) => packet,
                    Err(err) => {
                        log::warn!("[pluginsystem] transport.from_json parse failed: {}", err);
                        return Ok(Err(err.to_string()));
                    }
                };
                Ok(Ok(packet.encode_to_vec()))
            }
        }
    }
}

impl transport::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn send(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        data: Vec<u8>,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        let device_name = resolve_device_name(&device_addr).await;
        let params = json!({
            "plugin": plugin_name,
            "addr": device_addr.clone(),
            "deviceName": device_name,
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "request", params).await {
            return Ok(Err("permission denied".to_string()));
        }
        if !transport_protocol_supported(&device_addr).await {
            log::warn!(
                "[pluginsystem] transport.send only supports Xiaomi SARv2 devices for now: {}",
                device_addr
            );
            return Ok(Err("unsupported device protocol".to_string()));
        }
        let Ok(packet) = decode_pb_packet(&data) else {
            return Ok(Err("invalid protobuf packet".to_string()));
        };
        match send_xiaomi_pb_packet(&device_addr, packet).await {
            Ok(()) => Ok(Ok(())),
            Err(()) => Ok(Err("failed to enqueue packet".to_string())),
        }
    }

    async fn request(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        data: Vec<u8>,
    ) -> wasmtime::Result<Result<Vec<u8>, String>> {
        let (app_handle, plugin_name, permissions, generation) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.runtime_generation(),
            )
        });

        let device_name = resolve_device_name(&device_addr).await;
        let params = json!({
            "plugin": plugin_name.as_str(),
            "addr": device_addr.clone(),
            "deviceName": device_name,
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "request", params).await {
            return Ok(Err("permission denied".to_string()));
        }
        if !transport_protocol_supported(&device_addr).await {
            log::warn!(
                "[pluginsystem] transport.request only supports Xiaomi SARv2 devices for now: {}",
                device_addr
            );
            return Ok(Err("unsupported device protocol".to_string()));
        }

        let Ok(packet) = decode_pb_packet(&data) else {
            return Ok(Err("invalid protobuf packet".to_string()));
        };
        let protobuf_type_id = u32::try_from(packet.r#type).ok();
        let protobuf_packet_id = Some(packet.id);
        let Some(rx) = transport_runtime::register_request_waiter(
            plugin_name.as_str(),
            generation,
            device_addr.clone(),
            L2Channel::Pb as u32,
            protobuf_type_id,
            protobuf_packet_id,
        ) else {
            log::debug!(
                "[pluginsystem] transport.request rejected for stale response quarantine: plugin={}, generation={}",
                plugin_name,
                generation
            );
            return Ok(Err("plugin runtime restarted".to_string()));
        };

        if send_xiaomi_pb_packet(&device_addr, packet).await.is_err() {
            return Ok(Err("failed to enqueue packet".to_string()));
        }

        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(payload)) => Ok(Ok(payload)),
            Ok(Err(_)) => Ok(Err("response channel closed".to_string())),
            Err(_) => {
                log::warn!(
                    "[pluginsystem] transport.request timed out for {}",
                    device_addr
                );
                Ok(Err("request timed out".to_string()))
            }
        }
    }
}
