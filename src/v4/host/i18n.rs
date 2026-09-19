//! Level 4 的 i18n 接口。

use frontbridge::invoke_frontend;
use serde::{Deserialize, Serialize};
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::v4::bindings::astrobox::psys_host_v4::i18n;
use crate::v4::ctx::PluginCtxV4;

const FRONT_I18N_LOAD_JSON_METHOD: &str = "host/i18n/load_json";

#[derive(Debug, Serialize)]
struct LoadI18nJsonPayload {
    content: String,
}

#[derive(Debug, Deserialize)]
struct LoadI18nJsonAck {
    success: bool,
}

impl i18n::Host for PluginCtxV4 {}

impl i18n::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn load_json(
        accessor: &Accessor<PluginCtxV4, Self>,
        content: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_name().to_string())
        });

        let response = invoke_frontend::<LoadI18nJsonAck, _>(
            &app_handle,
            FRONT_I18N_LOAD_JSON_METHOD,
            LoadI18nJsonPayload { content },
        )
        .await;

        match response {
            Ok(ack) if ack.success => {
                log::info!("[plugin:{}] i18n.load-json loaded", plugin_name);
                Ok(Ok(()))
            }
            Ok(_) => {
                log::warn!(
                    "[plugin:{}] i18n.load-json rejected by frontend",
                    plugin_name
                );
                Ok(Err("rejected by frontend".to_string()))
            }
            Err(err) => {
                log::warn!(
                    "[plugin:{}] i18n.load-json invoke frontend failed: {}",
                    plugin_name,
                    err
                );
                Ok(Err(err.to_string()))
            }
        }
    }
}
