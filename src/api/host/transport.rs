use crate::bindings::astrobox::psys_host;
use crate::transport_runtime;
use anyhow::Error;
use corelib::device::xiaomi::{
    XiaomiDevice,
    packet::{
        cipher,
        v2::layer2::L2Channel,
    },
};
use pb::xiaomi::protocol::WearPacket;
use prost::Message;
use serde_json::json;
use std::time::Duration;
use wasmtime::component::{Accessor, FutureReader};

use super::{
    HostString, HostVec, PluginCtx,
    permission::{check_permission_declared, resolve_device_name},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

fn decode_pb_packet(data: &[u8]) -> Result<WearPacket, ()> {
    WearPacket::decode(data).map_err(|err| {
        log::warn!("[pluginsystem] invalid Xiaomi protobuf packet: {}", err);
    })
}

async fn transport_protocol_supported(device_addr: &str) -> bool {
    let device_addr = device_addr.to_string();
    corelib::ecs::with_rt_mut(move |rt| {
        rt.component_ref::<XiaomiDevice>(&device_addr)
            .map(|device| device.sar_version == 2)
            .unwrap_or(false)
    })
    .await
}

async fn send_xiaomi_pb_packet(device_addr: &str, packet: WearPacket) -> Result<(), ()> {
    let device_addr = device_addr.to_string();
    corelib::ecs::with_rt_mut(move |rt| {
        rt.with_device_mut(&device_addr, |world, entity| {
            let Some(dev) = world.get_mut::<XiaomiDevice>(entity) else {
                return Err(());
            };
            let bytes = cipher::encode_pb_packet(&*dev, packet, "PluginTransport::send");
            dev.sar.lock().enqueue(bytes);
            Ok(())
        })
        .unwrap_or(Err(()))
    })
    .await
}

impl psys_host::transport::Host for PluginCtx {
    fn to_json(
        &mut self,
        protocol: psys_host::transport::Protocol,
        data: HostVec<u8>,
    ) -> wasmtime::Result<HostString> {
        match protocol {
            psys_host::transport::Protocol::XiaomiVelaV5Protobuf => {
                let packet = match WearPacket::decode(data.as_slice()) {
                    Ok(packet) => packet,
                    Err(err) => {
                        log::warn!("[pluginsystem] transport.to_json decode failed: {}", err);
                        return Ok(HostString::default());
                    }
                };
                let json = serde_json::to_string_pretty(&packet).unwrap_or_default();
                Ok(HostString::from(json))
            }
        }
    }

    fn from_json(
        &mut self,
        protocol: psys_host::transport::Protocol,
        data: HostString,
    ) -> wasmtime::Result<Result<HostVec<u8>, ()>> {
        match protocol {
            psys_host::transport::Protocol::XiaomiVelaV5Protobuf => {
                let packet: WearPacket = match serde_json::from_str(data.as_str()) {
                    Ok(packet) => packet,
                    Err(err) => {
                        log::warn!("[pluginsystem] transport.from_json parse failed: {}", err);
                        return Ok(Err(()));
                    }
                };
                Ok(Ok(HostVec::from(packet.encode_to_vec())))
            }
        }
    }
}

impl psys_host::transport::HostWithStore for PluginCtx {
    fn send<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        data: HostVec<u8>,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let device_addr = device_addr.to_string();
                let data = data.as_slice().to_vec();
                let device_name = resolve_device_name(&device_addr).await;
                let params = json!({
                    "plugin": plugin_name,
                    "addr": device_addr.clone(),
                    "deviceName": device_name,
                });
                if !check_permission_declared(&app_handle, permissions.as_ref(), "request", params)
                    .await
                {
                    return Ok::<(), Error>(());
                }
                if !transport_protocol_supported(&device_addr).await {
                    log::warn!(
                        "[pluginsystem] transport.send only supports Xiaomi SARv2 devices for now: {}",
                        device_addr
                    );
                    return Ok::<(), Error>(());
                }
                let packet = match decode_pb_packet(&data) {
                    Ok(packet) => packet,
                    Err(()) => return Ok::<(), Error>(()),
                };
                let _ = send_xiaomi_pb_packet(&device_addr, packet).await;
                Ok::<(), Error>(())
            })
        });
        async move { future }
    }

    fn request<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        data: HostVec<u8>,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<HostVec<u8>, ()>>> + Send
    {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let device_addr = device_addr.to_string();
                let data = data.as_slice().to_vec();
                let device_name = resolve_device_name(&device_addr).await;
                let params = json!({
                    "plugin": plugin_name,
                    "addr": device_addr.clone(),
                    "deviceName": device_name,
                });
                if !check_permission_declared(&app_handle, permissions.as_ref(), "request", params)
                    .await
                {
                    return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(()));
                }
                if !transport_protocol_supported(&device_addr).await {
                    log::warn!(
                        "[pluginsystem] transport.request only supports Xiaomi SARv2 devices for now: {}",
                        device_addr
                    );
                    return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(()));
                }

                let packet = match decode_pb_packet(&data) {
                    Ok(packet) => packet,
                    Err(()) => return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(())),
                };
                let protobuf_type_id = u32::try_from(packet.r#type).ok();
                let protobuf_packet_id = Some(packet.id);
                let rx = transport_runtime::register_request_waiter(
                    device_addr.clone(),
                    L2Channel::Pb as u32,
                    protobuf_type_id,
                    protobuf_packet_id,
                );

                if send_xiaomi_pb_packet(&device_addr, packet).await.is_err() {
                    return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(()));
                }

                let response = match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
                    Ok(Ok(payload)) => payload,
                    Ok(Err(_)) => return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(())),
                    Err(_) => {
                        log::warn!(
                            "[pluginsystem] transport.request timed out for {}",
                            device_addr
                        );
                        return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(()));
                    }
                };

                Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Ok(HostVec::from(response)))
            })
        });
        async move { future }
    }
}
