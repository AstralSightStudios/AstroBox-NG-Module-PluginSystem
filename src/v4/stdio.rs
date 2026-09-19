//! Level 4 的 stdout / stderr 转接。
//!
//! 行缓冲与日志输出逻辑跟 Level 2/3 完全共用（`PluginStdioStream` 不依赖 wasmtime
//! 类型，`AsyncWrite` 来自 tokio，两个版本一致）；这里只补上 wasmtime-wasi 48 的
//! `StdoutStream` / `IsTerminal` 实现。

use tokio::io::AsyncWrite;
use wasmtime_wasi_v4::cli::{IsTerminal, StdoutStream};

use crate::plugin::{PluginStdioKind, PluginStdioStream};

#[derive(Clone)]
pub(crate) struct PluginStdioStreamV4(PluginStdioStream);

impl PluginStdioStreamV4 {
    pub(crate) fn new(plugin_name: &str, kind: PluginStdioKind) -> Self {
        Self(PluginStdioStream::new(plugin_name, kind))
    }
}

impl IsTerminal for PluginStdioStreamV4 {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for PluginStdioStreamV4 {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.0.clone())
    }
}
