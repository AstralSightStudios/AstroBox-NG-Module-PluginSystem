//! Level 4 的定时器接口。
//!
//! v4 里 set-timeout / set-interval / clear-timer 都是同步的：它们只是登记一个
//! tokio 任务，本来就不需要等待。到期后照旧通过 `dispatch_timer_event` 把 timer
//! 事件投递回插件（该函数按 API Level 自动选择对应的实例）。

use std::time::Duration;

use wasmtime_v4 as wasmtime;

use crate::api::host::timer::{TimerKind, build_timer_payload, dispatch_timer_event};
use crate::v4::bindings::astrobox::psys_host_v4::timer;
use crate::v4::ctx::PluginCtxV4;

impl timer::Host for PluginCtxV4 {
    fn set_timeout(&mut self, delay_ms: u64, payload: String) -> wasmtime::Result<u64> {
        let register_state = self.register_state();
        let plugin_name = self.plugin_name().to_string();
        let generation = self.runtime_generation();
        let timer_id = register_state.next_timer_id();
        let timer_state = register_state.clone();

        let handle = tokio::spawn(async move {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(delay_ms.max(1))).await;
            let timer_payload = build_timer_payload(timer_id, TimerKind::Timeout, payload);
            dispatch_timer_event(plugin_name, generation, timer_id, timer_payload).await;
            timer_state.remove_timer(timer_id);
        });
        register_state.insert_timer(timer_id, handle);
        Ok(timer_id)
    }

    fn set_interval(&mut self, interval_ms: u64, payload: String) -> wasmtime::Result<u64> {
        let register_state = self.register_state();
        let plugin_name = self.plugin_name().to_string();
        let generation = self.runtime_generation();
        let timer_id = register_state.next_timer_id();

        let handle = tokio::spawn(async move {
            tokio::task::yield_now().await;
            let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms.max(1)));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let timer_payload =
                    build_timer_payload(timer_id, TimerKind::Interval, payload.clone());
                dispatch_timer_event(plugin_name.clone(), generation, timer_id, timer_payload).await;
            }
        });
        register_state.insert_timer(timer_id, handle);
        Ok(timer_id)
    }

    fn clear_timer(&mut self, timer_id: u64) -> wasmtime::Result<()> {
        self.register_state().clear_timer(timer_id);
        Ok(())
    }
}
