//! 插件 CanopusConnect 消息处理导出的绑定。
//!
//! 和 `bindings_http` 同一个做法：单独一个只含导出的世界，对任意已实例化的 Level 4
//! 组件做一次可失败的查找，没实现的插件照常运行。`with` 把 `payload` 类型指回基础
//! 绑定，两边是同一个 Rust 类型。

wasmtime_v4::component::bindgen!({
    path: "wit",
    world: "psys-plugin-canopusconnect-export",
    wasmtime_crate: wasmtime_v4,
    with: {
        "astrobox:psys-host-v4/canopusconnect": crate::v4::bindings::astrobox::psys_host_v4::canopusconnect,
    },
    exports: {
        default: async | store,
    },
});
