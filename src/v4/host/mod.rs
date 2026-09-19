//! Level 4 的宿主接口实现（wasmtime 48 / WASI p3）。
//!
//! 这里只放**绑定层**：把 v4 的 bindgen 类型转成普通 Rust 类型，然后调用
//! 与版本无关的业务逻辑。凡是能共用的实现都从 `crate::api::host` 里复用，
//! 不在这边另起一套。

pub(crate) mod account;
pub(crate) mod browser;
pub(crate) mod clipboard;
pub(crate) mod device;
pub(crate) mod dialog;
pub(crate) mod event;
pub(crate) mod http_server;
pub(crate) mod i18n;
pub(crate) mod interconnect;
pub(crate) mod notification;
pub(crate) mod os;
pub(crate) mod provider_callback;
pub(crate) mod queue;
pub(crate) mod register;
pub(crate) mod thirdpartyapp;
pub(crate) mod timer;
pub(crate) mod transport;
pub(crate) mod ui;
pub(crate) mod watchface;
