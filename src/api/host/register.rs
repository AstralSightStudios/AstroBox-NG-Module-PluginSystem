use crate::bindings::astrobox::psys_host;
use crate::plugin::{
    CardRegistration, InterconnectRecvRegistration, ProviderRegistration, TransportRecvRegistration,
};
use wasmtime::component::{Access, FutureReader};

use super::{HostString, PluginCtx, ReadyFuture, permission::is_permission_declared};

impl psys_host::register::Host for PluginCtx {}

impl psys_host::register::HostWithStore for PluginCtx {
    fn register_transport_recv<T>(
        mut store: Access<'_, T, Self>,
        addr: HostString,
        filter: psys_host::register::TransportRecvFiler,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let plugin_name = store.get().plugin_name().to_string();
        let register_state = store.get().register_state();
        let permissions = store.get().permissions();
        let result = if check_declared_registration(
            &plugin_name,
            permissions.as_ref(),
            "register_transport_recv",
        ) {
            register_state.register_transport_recv_sync(TransportRecvRegistration {
                addr: addr.to_string(),
                filter,
            });
            Ok(())
        } else {
            Err(())
        };
        let future = FutureReader::new(&mut store, ReadyFuture::ok(result));
        future.expect("failed to create host future reader")
    }

    fn register_interconnect_recv<T>(
        mut store: Access<'_, T, Self>,
        addr: HostString,
        pkg_name: HostString,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let plugin_name = store.get().plugin_name().to_string();
        let register_state = store.get().register_state();
        let permissions = store.get().permissions();
        let result = if check_declared_registration(
            &plugin_name,
            permissions.as_ref(),
            "register_interconnect_recv",
        ) {
            register_state.register_interconnect_recv_sync(InterconnectRecvRegistration {
                addr: addr.to_string(),
                pkg_name: pkg_name.to_string(),
            });
            Ok(())
        } else {
            Err(())
        };
        let future = FutureReader::new(&mut store, ReadyFuture::ok(result));
        future.expect("failed to create host future reader")
    }

    fn register_deeplink_action<T>(
        mut store: Access<'_, T, Self>,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let plugin_name = store.get().plugin_name().to_string();
        let register_state = store.get().register_state();
        let permissions = store.get().permissions();
        let result = if check_declared_registration(
            &plugin_name,
            permissions.as_ref(),
            "register_deeplink_action",
        ) && register_state.try_register_deeplink_sync()
        {
            Ok(())
        } else {
            Err(())
        };
        let future = FutureReader::new(&mut store, ReadyFuture::ok(result));
        future.expect("failed to create host future reader")
    }

    fn register_provider<T>(
        mut store: Access<'_, T, Self>,
        name: HostString,
        provider_type: psys_host::register::ProviderType,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let plugin_name = store.get().plugin_name().to_string();
        let register_state = store.get().register_state();
        let permissions = store.get().permissions();
        let result =
            if check_declared_registration(&plugin_name, permissions.as_ref(), "register_provider")
            {
                register_state.register_provider_sync(ProviderRegistration {
                    name: name.to_string(),
                    provider_type,
                });
                Ok(())
            } else {
                Err(())
            };
        let future = FutureReader::new(&mut store, ReadyFuture::ok(result));
        future.expect("failed to create host future reader")
    }

    fn register_card<T>(
        mut store: Access<'_, T, Self>,
        card_type: psys_host::register::CardType,
        id: HostString,
        name: HostString,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let register_state = store.get().register_state();
        register_state.register_card_sync(CardRegistration {
            card_type,
            id: id.to_string(),
            name: name.to_string(),
        });
        let future = FutureReader::new(&mut store, ReadyFuture::ok(Ok(())));
        future.expect("failed to create host future reader")
    }
}

fn check_declared_registration(plugin_name: &str, permissions: &[String], operation: &str) -> bool {
    if is_permission_declared(permissions, operation) {
        log::info!(
            "[plugin:{}] registration permission '{}' declared",
            plugin_name,
            operation
        );
        true
    } else {
        log::warn!(
            "[plugin:{}] permission '{}' not declared by plugin",
            plugin_name,
            operation
        );
        false
    }
}
