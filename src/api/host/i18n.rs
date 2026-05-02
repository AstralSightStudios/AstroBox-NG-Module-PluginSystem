use anyhow::Error;
use frontbridge::invoke_frontend;
use serde::{Deserialize, Serialize};
use wasmtime::component::{Access, FutureReader};

use crate::bindings::astrobox::psys_host;

use super::{HostString, PluginCtx};

const FRONT_I18N_LOAD_JSON_METHOD: &str = "host/i18n/load_json";

#[derive(Debug, Serialize)]
struct LoadI18nJsonPayload {
    content: String,
}

#[derive(Debug, Deserialize)]
struct LoadI18nJsonAck {
    success: bool,
}

impl psys_host::i18n::Host for PluginCtx {}

impl psys_host::i18n::HostWithStore for PluginCtx {
    fn load_json<T>(
        mut store: Access<'_, T, Self>,
        content: HostString,
    ) -> FutureReader<core::result::Result<(), ()>> {
        let app_handle = store.get().app_handle();
        let plugin_name = store.get().plugin_name().to_string();
        let future = {
            FutureReader::new(
                &mut store,
                crate::api::host::AnyhowFuture(async move {
                    let payload = LoadI18nJsonPayload {
                        content: content.to_string(),
                    };

                    let response = invoke_frontend::<LoadI18nJsonAck, _>(
                        &app_handle,
                        FRONT_I18N_LOAD_JSON_METHOD,
                        payload,
                    )
                    .await;

                    match response {
                        Ok(ack) if ack.success => {
                            log::info!("[plugin:{}] i18n.load-json loaded", plugin_name);
                            Ok::<core::result::Result<(), ()>, Error>(Ok(()))
                        }
                        Ok(_) => {
                            log::warn!(
                                "[plugin:{}] i18n.load-json rejected by frontend",
                                plugin_name
                            );
                            Ok::<core::result::Result<(), ()>, Error>(Err(()))
                        }
                        Err(err) => {
                            log::warn!(
                                "[plugin:{}] i18n.load-json invoke frontend failed: {}",
                                plugin_name,
                                err
                            );
                            Ok::<core::result::Result<(), ()>, Error>(Err(()))
                        }
                    }
                }),
            )
        };
        future.expect("failed to create host future reader")
    }
}
