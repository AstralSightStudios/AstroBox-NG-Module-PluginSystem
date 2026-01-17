use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use serde_json::json;
use wasmtime::component::{Accessor, FutureReader};

use super::{
    HostString, HostVec, PluginCtx,
    permission::{check_permission_declared, resolve_device_name},
};

impl psys_host::transport::Host for PluginCtx {
    fn to_json(
        &mut self,
        _protocol: psys_host::transport::Protocol,
        _data: HostVec<u8>,
    ) -> wasmtime::Result<HostString> {
        Ok(HostString::default())
    }

    fn from_json(
        &mut self,
        _protocol: psys_host::transport::Protocol,
        _data: HostString,
    ) -> wasmtime::Result<Result<HostVec<u8>, ()>> {
        Ok(Ok(HostVec::new()))
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
                let _ = data;
                let device_name = resolve_device_name(&device_addr).await;
                let params = json!({
                    "plugin": plugin_name,
                    "addr": device_addr,
                    "deviceName": device_name,
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "request",
                    params,
                )
                .await
                {
                    return Ok::<(), Error>(());
                }
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
                let _ = data;
                let device_name = resolve_device_name(&device_addr).await;
                let params = json!({
                    "plugin": plugin_name,
                    "addr": device_addr,
                    "deviceName": device_name,
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "request",
                    params,
                )
                .await
                {
                    return Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Err(()));
                }
                Ok::<core::result::Result<HostVec<u8>, ()>, Error>(Ok(HostVec::new()))
            })
        });
        async move { future }
    }
}
