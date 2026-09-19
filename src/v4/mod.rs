//! API Level 4 运行时。
//!
//! Level 4 插件跑在**另一份 wasmtime**（48.x，WASI p3）上，与 Level 2/3 使用的
//! 38.x 完全隔离。两者在同一个进程里共存，互不共享任何 wasmtime 类型：
//! `Engine` / `Store` / `Linker` / `Accessor` 这些类型都是版本相关的，所以
//! host 接口的绑定层必须各写一份，只有纯业务逻辑可以复用。
//!
//! 本模块内的 `wasmtime` 一律指 48.x，通过 Cargo 的 `package` 重命名引入为
//! `wasmtime_v4`；模块内统一 `use wasmtime_v4 as wasmtime;`，这样 `bindgen!`
//! 生成的代码路径也能直接对上。

pub(crate) mod bindings;
pub(crate) mod bindings_http;
pub(crate) mod ctx;
pub(crate) mod engine;
pub(crate) mod host;
pub(crate) mod http_server;
pub(crate) mod runtime;
pub(crate) mod stdio;
