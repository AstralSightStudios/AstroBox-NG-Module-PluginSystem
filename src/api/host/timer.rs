use crate::bindings::{astrobox::psys_host, exports::astrobox::psys_plugin};
use anyhow::Error;
use serde_json::json;
use std::time::Duration;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

enum TimerKind {
    Timeout,
    Interval,
}

impl TimerKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Interval => "interval",
        }
    }
}

fn build_timer_payload(timer_id: u64, kind: TimerKind, payload: String) -> String {
    json!({
        "timerId": timer_id,
        "kind": kind.as_str(),
        "payload": payload,
    })
    .to_string()
}

async fn dispatch_timer_event(plugin_name: String, timer_id: u64, payload: String) {
    let result = crate::with_plugin_manager_async({
        let plugin_name = plugin_name.clone();
        let payload = payload.clone();
        move |pm| {
            let plugin_name = plugin_name.clone();
            let payload = payload.clone();
            Box::pin(async move {
                if let Some(plugin) = pm.plugins.get(&plugin_name) {
                    if plugin.state.disabled || !plugin.state.loaded {
                        log::debug!(
                            "Timer {} fired for inactive plugin {}",
                            timer_id,
                            plugin_name
                        );
                        return;
                    }

                    if let Err(err) = plugin
                        .runtime
                        .dispatch_event(psys_plugin::event::EventType::Timer, payload)
                        .await
                    {
                        log::error!(
                            "Failed to deliver timer {} to {}: {err}",
                            timer_id,
                            plugin_name
                        );
                    }
                } else {
                    log::warn!(
                        "Timer {} fired for missing plugin {}",
                        timer_id,
                        plugin_name
                    );
                }
            })
        }
    })
    .await;

    if let Err(err) = result {
        log::error!(
            "Failed to dispatch timer {} for {}: {err}",
            timer_id,
            plugin_name
        );
    }
}

impl psys_host::timer::Host for PluginCtx {}

impl psys_host::timer::HostWithStore for PluginCtx {
    fn set_timeout<T>(
        accessor: &Accessor<T, Self>,
        delay_ms: u64,
        payload: HostString,
    ) -> impl core::future::Future<Output = FutureReader<u64>> + Send {
        let instance = accessor.instance();
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let timer_id = register_state.next_timer_id();
                let payload = payload.to_string();
                let timer_state = register_state.clone();
                let plugin_name = plugin_name.clone();
                let handle = tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    let delay_ms = delay_ms.max(1);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    let timer_payload =
                        build_timer_payload(timer_id, TimerKind::Timeout, payload);
                    dispatch_timer_event(plugin_name, timer_id, timer_payload).await;
                    timer_state.remove_timer(timer_id);
                });
                register_state.insert_timer(timer_id, handle);
                Ok::<u64, Error>(timer_id)
            })
        });
        async move { future }
    }

    fn set_interval<T>(
        accessor: &Accessor<T, Self>,
        interval_ms: u64,
        payload: HostString,
    ) -> impl core::future::Future<Output = FutureReader<u64>> + Send {
        let instance = accessor.instance();
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let timer_id = register_state.next_timer_id();
                let payload = payload.to_string();
                let plugin_name = plugin_name.clone();
                let handle = tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    let interval_ms = interval_ms.max(1);
                    let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
                    ticker.tick().await;
                    loop {
                        ticker.tick().await;
                        let timer_payload =
                            build_timer_payload(timer_id, TimerKind::Interval, payload.clone());
                        dispatch_timer_event(plugin_name.clone(), timer_id, timer_payload).await;
                    }
                });
                register_state.insert_timer(timer_id, handle);
                Ok::<u64, Error>(timer_id)
            })
        });
        async move { future }
    }

    fn clear_timer<T>(
        accessor: &Accessor<T, Self>,
        timer_id: u64,
    ) -> impl core::future::Future<Output = FutureReader<()>> + Send {
        let instance = accessor.instance();
        let register_state = accessor.with(|mut access| access.get().register_state());
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                register_state.clear_timer(timer_id);
                Ok::<(), Error>(())
            })
        });
        async move { future }
    }
}
