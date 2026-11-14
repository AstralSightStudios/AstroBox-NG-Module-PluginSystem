use crate::bindings::astrobox::psys_host;
use anyhow::{Error, anyhow};
use corelib::device::xiaomi::components::thirdparty_app::{AppInfo, ThirdpartyAppSystem};
use log::error;
use serde_json::json;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx, permission::check_permission_declared};

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::interconnect::HostWithStore for PluginCtx {
    fn send_qaic_message<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        pkg_name: HostString,
        data: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let permissions = accessor.with(|mut access| access.get().permissions());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let device_addr = device_addr.to_string();
                let pkg_name = pkg_name.to_string();
                let payload = data.to_string().into_bytes();

                let params = json!({
                    "plugin": plugin_name,
                    "addr": device_addr.clone(),
                    "pkgName": pkg_name.clone(),
                });
                if !check_permission_declared(
                    &app_handle,
                    permissions.as_ref(),
                    "interconnect",
                    params,
                )
                .await
                {
                    return Ok::<core::result::Result<(), ()>, Error>(Err(()));
                }

                match send_qaic_message_impl(device_addr, pkg_name, payload).await {
                    Ok(()) => Ok::<core::result::Result<(), ()>, Error>(Ok(())),
                    Err(err) => {
                        error!("Failed to send QAIC message to package: {err:?}");
                        Ok::<core::result::Result<(), ()>, Error>(Err(()))
                    }
                }
            })
        });
        async move { future }
    }
}

pub(crate) async fn send_qaic_message_impl(
    device_addr: String,
    pkg_name: String,
    payload: Vec<u8>,
) -> Result<(), Error> {
    let app_info = super::thirdpartyapp::resolve_quick_app(&device_addr, &pkg_name).await?;
    dispatch_message(device_addr, app_info, payload).await
}

async fn dispatch_message(
    device_addr: String,
    app_info: AppInfo,
    payload: Vec<u8>,
) -> Result<(), Error> {
    corelib::ecs::with_rt_mut(move |rt| -> Result<(), Error> {
        rt.with_device_mut(&device_addr, |world, entity| {
            let mut system = world
                .get_mut::<ThirdpartyAppSystem>(entity)
                .ok_or_else(|| anyhow!("ThirdpartyApp system not found on {}", device_addr))?;
            system.send_phone_message(&app_info, payload);
            Ok(())
        })
        .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?
    })
    .await
}
