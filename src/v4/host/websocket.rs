//! Level 4 的 WebSocket 客户端接口。实际的连接管理在 `crate::v4::websocket`。
//!
//! 和发 HTTP 请求（wasi-http）一样不设权限：能连 WebSocket 并不比能发 HTTP 请求多出
//! 什么能力，再弹一次授权只会教用户无脑点「允许」。

use wasmtime::component::Accessor;
use wasmtime_v4 as wasmtime;

use crate::v4::bindings::astrobox::psys_host_v4::websocket;
use crate::v4::ctx::PluginCtxV4;

impl websocket::Host for PluginCtxV4 {}

impl websocket::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn connect(
        accessor: &Accessor<PluginCtxV4, Self>,
        options: websocket::ConnectOptions,
    ) -> wasmtime::Result<Result<websocket::ConnectionInfo, String>> {
        let (registry, plugin_name, push_tx) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.websockets(),
                ctx.plugin_name().to_string(),
                ctx.ws_dispatch_tx(),
            )
        });
        Ok(registry.connect(plugin_name, options, push_tx).await)
    }

    async fn send(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        message: websocket::Message,
    ) -> wasmtime::Result<Result<(), String>> {
        let registry = accessor.with(|mut access| access.get().websockets());
        Ok(registry.send(id, message).await)
    }

    async fn receive(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        timeout_ms: Option<u64>,
    ) -> wasmtime::Result<Result<Option<websocket::Incoming>, String>> {
        let registry = accessor.with(|mut access| access.get().websockets());
        Ok(registry.receive(id, timeout_ms).await)
    }

    async fn close(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        code: Option<u16>,
        reason: Option<String>,
    ) -> wasmtime::Result<Result<(), String>> {
        let registry = accessor.with(|mut access| access.get().websockets());
        Ok(registry.close(id, code, reason).await)
    }
}
