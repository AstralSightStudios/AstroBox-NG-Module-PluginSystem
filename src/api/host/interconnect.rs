use crate::bindings::astrobox::psys_host;
use anyhow::{Error, anyhow};
use corelib::{
    device::xiaomi::{
        XiaomiDevice,
        components::{
            resource::ResourceComponent,
            thirdparty_app::{AppInfo, ThirdpartyAppComponent, ThirdpartyAppSystem},
        },
    },
    ecs::{entity::EntityExt, logic_component::LogicComponent},
};
use log::error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

impl psys_host::interconnect::Host for PluginCtx {}

impl psys_host::interconnect::HostWithStore for PluginCtx {
    fn send_qaic_message<T>(
        accessor: &Accessor<T, Self>,
        device_addr: HostString,
        pkg_name: HostString,
        data: HostString,
    ) -> impl core::future::Future<Output = FutureReader<core::result::Result<(), ()>>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let device_addr = device_addr.to_string();
                let pkg_name = pkg_name.to_string();
                let payload = data.to_string().into_bytes();

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

async fn send_qaic_message_impl(
    device_addr: String,
    pkg_name: String,
    payload: Vec<u8>,
) -> Result<(), Error> {
    let app_info = resolve_app_info(&device_addr, &pkg_name).await?;
    dispatch_message(device_addr, app_info, payload).await
}

async fn resolve_app_info(device_addr: &str, pkg_name: &str) -> Result<AppInfo, Error> {
    let device_addr = device_addr.to_string();
    let pkg_name = pkg_name.to_string();
    corelib::ecs::with_rt_mut(move |rt| -> Result<AppInfo, Error> {
        let dev = rt
            .find_entity_by_id_mut::<XiaomiDevice>(&device_addr)
            .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?;

        let resource_comp = dev
            .get_component_as_mut::<ResourceComponent>(ResourceComponent::ID)
            .map_err(|err| anyhow!("Resource component unavailable: {:?}", err))?;

        resource_comp
            .quick_apps
            .iter()
            .find(|item| item.package_name == pkg_name)
            .map(|item| AppInfo {
                package_name: item.package_name.clone(),
                fingerprint: item.fingerprint.clone(),
            })
            .ok_or_else(|| anyhow!("Quick app {} not found on {}", pkg_name, device_addr))
    })
    .await
}

async fn dispatch_message(
    device_addr: String,
    app_info: AppInfo,
    payload: Vec<u8>,
) -> Result<(), Error> {
    corelib::ecs::with_rt_mut(move |rt| -> Result<(), Error> {
        let dev = rt
            .find_entity_by_id_mut::<XiaomiDevice>(&device_addr)
            .ok_or_else(|| anyhow!("Device not found: {}", device_addr))?;

        let component = dev
            .get_component_as_mut::<ThirdpartyAppComponent>(ThirdpartyAppComponent::ID)
            .map_err(|err| anyhow!("ThirdpartyApp component unavailable: {:?}", err))?;

        let system = component
            .system_mut()
            .as_any_mut()
            .downcast_mut::<ThirdpartyAppSystem>()
            .ok_or_else(|| anyhow!("ThirdpartyApp system not found on {}", device_addr))?;

        system.send_phone_message(&app_info, payload);
        Ok(())
    })
    .await
}
