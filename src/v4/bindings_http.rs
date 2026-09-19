//! 插件 HTTP 处理导出的绑定。
//!
//! 单独一个只含导出的世界，好处是可以对**任意**已实例化的 Level 4 组件做一次
//! 可失败的查找：实现了 `astrobox:psys-plugin-v4/http` 的插件能拿到 handler，
//! 没实现的照常运行，不影响实例化。
//!
//! `with` 把请求/响应类型指回基础绑定，两边用的是同一组 Rust 类型，不需要转换。

wasmtime_v4::component::bindgen!({
    path: "wit",
    world: "psys-plugin-http-export",
    wasmtime_crate: wasmtime_v4,
    with: {
        "astrobox:psys-host-v4/http-server": crate::v4::bindings::astrobox::psys_host_v4::http_server,
    },
    exports: {
        default: async | store,
    },
});
