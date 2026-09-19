use account::models::{BindingStatus, PublicAccountProfile};
use serde_json::json;
use wasmtime_v4::{self as wasmtime, component::Accessor};

use crate::{
    api::host::permission::check_permission_declared,
    v4::{bindings::astrobox::psys_host_v4::account as bindings, ctx::PluginCtxV4},
};

impl bindings::Host for PluginCtxV4 {}

impl bindings::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn get_current(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<Result<Option<bindings::Profile>, String>> {
        let (app, plugin, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });
        if !check_permission_declared(
            &app,
            &permissions,
            "account.profile",
            json!({"plugin": plugin}),
        )
        .await
        {
            return Ok(Err("permission denied".into()));
        }
        let Some(provider) = account::get_account_provider("astrobox").await else {
            return Ok(Err("AstroBox account provider is not initialized".into()));
        };
        Ok(match provider.public_profile().await {
            Ok(value) => Ok(value.map(to_profile)),
            // Provider errors may contain HTTP response bodies; don't expose those to plugins.
            Err(_) => Err("could not read account profile; session may have changed, retry".into()),
        })
    }
}

fn to_profile(value: PublicAccountProfile) -> bindings::Profile {
    bindings::Profile {
        id: value.id,
        name: value.name,
        username: value.username,
        avatar: value.avatar,
        source: value.source,
        bindings: value
            .bindings
            .into_iter()
            .map(|value| bindings::Binding {
                provider: value.provider,
                status: match value.status {
                    BindingStatus::Linked => bindings::BindingStatus::Linked,
                    BindingStatus::Unlinked => bindings::BindingStatus::Unlinked,
                    BindingStatus::Unavailable => bindings::BindingStatus::Unavailable,
                },
                id: value.id,
                username: value.username,
                display_name: value.display_name,
                avatar: value.avatar,
            })
            .collect(),
    }
}
