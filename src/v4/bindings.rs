//! Level 4 的 wasmtime 绑定（wasmtime 48 / WASI p3）。
//!
//! 与 `crate::bindings` / `crate::bindings_v3` 的两点关键差别：
//!
//!  * 走的是**另一个 wasmtime crate**。Cargo 里以 `package = "wasmtime"` 重命名
//!    引入为 `wasmtime_v4`，因此必须给 `bindgen!` 传 `wasmtime_crate`，否则生成的
//!    代码会去找 38.x 那个 `wasmtime`，类型对不上。
//!
//!  * WIT 里会等待的接口都是 `async func`，这里统一配成 `async | store`，生成
//!    `async fn foo(store: Access<'_, T, Self>, ..) -> Result<..>`。`store`（即
//!    concurrent host function）很重要：它不独占 Store，宿主等待期间
//!    `run_concurrent` 事件循环仍能推进其它任务；若用不带 `store` 的 `async`，
//!    宿主调用会独占 Store 把整个实例的并发全卡住。
//!
//!  * 纯本地操作（element 构建器、render、timer 等）保持 `trappable` 默认形态，
//!    也就是普通的 `&mut self` 方法，与 v3 写法一致。

wasmtime_v4::component::bindgen!({
    path: "wit",
    world: "psys-world-v4",
    wasmtime_crate: wasmtime_v4,
    with: {
        // Element 是纯数据结构，与 wasmtime 版本无关，直接复用 v3 的那份。
        "astrobox:psys-host-v4/ui.element": crate::api::host::v3::ui::Element,
    },
    imports: {
        "astrobox:psys-host-v4/os.arch": async | store | trappable,
        "astrobox:psys-host-v4/os.hostname": async | store | trappable,
        "astrobox:psys-host-v4/os.locale": async | store | trappable,
        "astrobox:psys-host-v4/os.platform": async | store | trappable,
        "astrobox:psys-host-v4/os.version": async | store | trappable,
        "astrobox:psys-host-v4/os.astrobox-language": async | store | trappable,
        "astrobox:psys-host-v4/os.appearance": async | store | trappable,
        "astrobox:psys-host-v4/os.timezone-offset-minutes": async | store | trappable,

        "astrobox:psys-host-v4/transport.send": async | store | trappable,
        "astrobox:psys-host-v4/transport.request": async | store | trappable,

        "astrobox:psys-host-v4/clipboard.read-text": async | store | trappable,
        "astrobox:psys-host-v4/clipboard.write-text": async | store | trappable,

        "astrobox:psys-host-v4/dialog.show-dialog": async | store | trappable,
        "astrobox:psys-host-v4/dialog.pick-file": async | store | trappable,
        "astrobox:psys-host-v4/dialog.save-file-start": async | store | trappable,
        "astrobox:psys-host-v4/dialog.save-file-write-chunk": async | store | trappable,
        "astrobox:psys-host-v4/dialog.save-file-finish": async | store | trappable,
        "astrobox:psys-host-v4/dialog.save-file-abort": async | store | trappable,

        "astrobox:psys-host-v4/ui.get-render-size": async | store | trappable,

        "astrobox:psys-host-v4/device.get-device-list": async | store | trappable,
        "astrobox:psys-host-v4/device.get-connected-device-list": async | store | trappable,
        "astrobox:psys-host-v4/device.disconnect-device": async | store | trappable,

        "astrobox:psys-host-v4/register.register-transport-recv": async | store | trappable,
        "astrobox:psys-host-v4/register.register-interconnect-recv": async | store | trappable,
        "astrobox:psys-host-v4/register.register-deeplink-action": async | store | trappable,
        "astrobox:psys-host-v4/register.register-provider": async | store | trappable,
        "astrobox:psys-host-v4/register.register-card": async | store | trappable,

        "astrobox:psys-host-v4/interconnect.send-qaic-message": async | store | trappable,

        "astrobox:psys-host-v4/thirdpartyapp.launch-qa": async | store | trappable,
        "astrobox:psys-host-v4/thirdpartyapp.get-thirdparty-app-list": async | store | trappable,

        "astrobox:psys-host-v4/watchface.get-watchface-list": async | store | trappable,
        "astrobox:psys-host-v4/watchface.set-current-watchface": async | store | trappable,

        "astrobox:psys-host-v4/i18n.load-json": async | store | trappable,

        "astrobox:psys-host-v4/notification.send": async | store | trappable,
        "astrobox:psys-host-v4/notification.remove": async | store | trappable,

        "astrobox:psys-host-v4/os.device-id": async | store | trappable,
        "astrobox:psys-host-v4/account.get-current": async | store | trappable,

        default: trappable,
    },
    exports: {
        default: async | store,
    },
});
