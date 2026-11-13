//! 插件资源安装处理导出的绑定。
//!
//! 和 `bindings_http` / `bindings_canopusconnect` 同一个做法：单独一个只含导出的世界，
//! 对任意已实例化的 Level 4 组件做一次可失败的查找，没实现的插件照常运行。
//! 请求/结果类型只在插件包里，没有要指回基础绑定的宿主类型。

wasmtime_v4::component::bindgen!({
    path: "wit",
    world: "psys-plugin-resource-install-export",
    wasmtime_crate: wasmtime_v4,
    exports: {
        default: async | store,
    },
});
