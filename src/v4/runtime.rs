//! Level 4 的插件运行时。
//!
//! 与 Level 2/3 的两点结构性差别：
//!
//! **1. 独立线程池。** Level 4 用的是另一个大版本的 wasmtime，而 macOS 上 wasmtime
//! 的陷阱处理走的是**线程级 Mach 异常端口**，且每个线程只在首次进入 wasm 时注册一次。
//! 如果同一个 OS 线程先后跑过 38.x 和 48.x 的 wasm，后注册的那份会顶掉前一份，之后
//! 前者的陷阱（aarch64 上 `unreachable` 会触发 EXC_BAD_INSTRUCTION，插件 panic 就是
//! 这条路径）就会落到不认识它的处理器手里，直接把整个进程带走。因此 Level 4 固定跑在
//! 自己的 tokio 运行时上，两个版本的 wasm 永远不共用 OS 线程。
//!
//! **2. 常驻 `run_concurrent` driver。** v4 的导出是 `async func`，调用要在
//! `Store::run_concurrent` 的事件循环里发起。每个插件一个 driver 任务独占自己的
//! Store，事件通过命令通道投进去，并发执行；一个插件阻塞只挂起它自己，不影响别人，
//! 也不需要 Level 2/3 那个进程级 `PLUGIN_EXEC_LOCK`。

use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use futures_util::future::Either;
use futures_util::stream::FuturesUnordered;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::{Mutex, mpsc, oneshot};
use wasmtime_v4 as wasmtime;

use wasmtime::component::{Component, Linker};
use wasmtime::{Store, UpdateDeadline};
use wasmtime_wasi_v4::{FsPerms, WasiCtx, WasiCtxBuilder};

use crate::manifest::PluginManifest;
use crate::plugin::PluginRegisterState;
use crate::v4::bindings::PsysWorldV4;
use crate::v4::bindings_http::PsysPluginHttpExport;
use crate::v4::http_server::{HttpDispatch, HttpServerRegistry};
use crate::v4::bindings::astrobox::psys_host_v4::ui as ui_bindings;
use crate::v4::bindings::exports::astrobox::psys_plugin_v4::event as plugin_event;
use crate::v4::ctx::PluginCtxV4;
use crate::v4::stdio::PluginStdioStreamV4;
use crate::v4::engine::{PLUGIN_EPOCH_TICKS_PER_CALL, create_engine, register_epoch_engine};

const PRECOMPILE_INDEX_FILE: &str = "precompiled-index-v4.json";

/// Level 4 专用的 tokio 运行时。
///
/// 独立线程池是安全要求而不是性能优化，理由见模块头注释。
fn v4_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("astrobox-psys-v4")
            .enable_all()
            .build()
            .expect("failed to build the API level 4 plugin runtime")
    })
}

#[derive(Default, Serialize, Deserialize)]
struct PrecompiledIndex {
    #[serde(default)]
    entries: HashMap<String, PrecompiledRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PrecompiledRecord {
    wasm_sha256: String,
    engine_hash: u64,
}

impl PrecompiledIndex {
    fn load(root: &Path) -> Self {
        let path = root.join(PRECOMPILE_INDEX_FILE);
        let Ok(data) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(PRECOMPILE_INDEX_FILE);
        let data = serde_json::to_string_pretty(self)
            .context("failed to serialize the level 4 precompile index")?;
        fs::write(&path, data)
            .with_context(|| format!("failed to persist precompile index to {}", path.display()))
    }
}

fn precompiled_artifact_path(entry_wasm: &Path) -> PathBuf {
    // 与 Level 2/3 的 `.cwasm` 分开：两个 wasmtime 版本的产物互不兼容。
    entry_wasm.with_extension("cwasm4")
}

pub(crate) fn purge_precompiled_component(plugin_dir: &Path, manifest: &PluginManifest) {
    let root = plugin_dir.parent().unwrap_or(plugin_dir);
    let artifact = precompiled_artifact_path(&manifest.entry_wasm_path(plugin_dir));
    if artifact.exists() {
        if let Err(err) = fs::remove_file(&artifact) {
            log::warn!(
                "Failed to remove level 4 precompiled artifact {}: {err}",
                artifact.display()
            );
        }
    }

    let mut index = PrecompiledIndex::load(root);
    if index.entries.remove(&manifest.name).is_some() {
        let _ = index.save(root);
    }
}

fn ensure_precompiled_component(
    engine: &wasmtime::Engine,
    plugin_dir: &Path,
    manifest: &PluginManifest,
    entry_wasm: &Path,
) -> Result<PathBuf> {
    let root = plugin_dir.parent().unwrap_or(plugin_dir).to_path_buf();
    let mut index = PrecompiledIndex::load(&root);

    let wasm_bytes = fs::read(entry_wasm).with_context(|| {
        format!(
            "failed to read plugin wasm component {}",
            entry_wasm.display()
        )
    })?;
    let wasm_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&wasm_bytes);
        hex::encode(hasher.finalize())
    };
    let engine_hash = {
        let mut hasher = DefaultHasher::new();
        engine.precompile_compatibility_hash().hash(&mut hasher);
        hasher.finish()
    };
    let artifact_path = precompiled_artifact_path(entry_wasm);

    let needs_recompile = index
        .entries
        .get(&manifest.name)
        .map(|cached| cached.wasm_sha256 != wasm_hash || cached.engine_hash != engine_hash)
        .unwrap_or(true)
        || !artifact_path.is_file();

    if needs_recompile {
        log::info!(
            "[plugin:{}] Precompiling wasm (api level 4) for faster startup...",
            manifest.name
        );
        let compiled = engine
            .precompile_component(&wasm_bytes)
            .map_err(anyhow::Error::from)
            .with_context(|| {
                format!(
                    "failed to precompile component for plugin {}",
                    manifest.name
                )
            })?;
        fs::write(&artifact_path, compiled).with_context(|| {
            format!(
                "failed to write precompiled artifact for plugin {} at {}",
                manifest.name,
                artifact_path.display()
            )
        })?;
        index.entries.insert(
            manifest.name.clone(),
            PrecompiledRecord {
                wasm_sha256: wasm_hash,
                engine_hash,
            },
        );
        index.save(&root)?;
    }

    Ok(artifact_path)
}

fn load_precompiled_component(
    engine: &wasmtime::Engine,
    artifact_path: &Path,
) -> Result<Component> {
    // 与 Level 2/3 同样的理由：Windows 上从字节加载，避免占着 .cwasm 的句柄导致
    // 热重载改名失败。
    let artifact = fs::read(artifact_path).with_context(|| {
        format!(
            "Failed to read precompiled plugin component: {}",
            artifact_path.display()
        )
    })?;
    unsafe {
        Component::deserialize(engine, artifact)
            .map_err(anyhow::Error::from)
            .with_context(|| {
                format!(
                    "Failed to load precompiled plugin component: {}",
                    artifact_path.display()
                )
            })
    }
}

/// 投给 driver 的一条工作。
pub(crate) enum V4Command {
    Event {
        event_type: plugin_event::EventType,
        payload: String,
    },
    UiEvent {
        event_id: String,
        event: ui_bindings::Event,
        payload: String,
    },
    UiRender {
        element_id: String,
    },
    CardRender {
        card_id: String,
    },
}

struct DriverHandle {
    cmd_tx: mpsc::UnboundedSender<V4Command>,
    task: tokio::task::JoinHandle<()>,
}

pub(crate) struct PluginRuntimeV4 {
    name: String,
    engine: wasmtime::Engine,
    component: Component,
    plugin_root: PathBuf,
    app_handle: AppHandle,
    register_state: Arc<PluginRegisterState>,
    permissions: Arc<Vec<String>>,
    _epoch_owner: Arc<()>,
    driver: Mutex<Option<DriverHandle>>,
    http_servers: Arc<HttpServerRegistry>,
}

impl PluginRuntimeV4 {
    pub(crate) fn initialise(
        path: &Path,
        manifest: &PluginManifest,
        app_handle: AppHandle,
        register_state: Arc<PluginRegisterState>,
        permissions: Arc<Vec<String>>,
    ) -> Result<Self> {
        let entry_path = manifest.entry_wasm_path(path);
        if !entry_path.is_file() {
            return Err(corelib::anyhow_site!(
                "plugin entry file does not exist: {}",
                entry_path.display()
            ));
        }

        log::info!(
            "[plugin:{}] Creating wasmtime 48 engine (api level 4)...",
            manifest.name
        );
        let engine = create_engine()?;
        let artifact_path = ensure_precompiled_component(&engine, path, manifest, &entry_path)?;
        let component = load_precompiled_component(&engine, &artifact_path)?;
        let epoch_owner = register_epoch_engine(&engine);

        Ok(Self {
            name: manifest.name.clone(),
            engine,
            component,
            plugin_root: path.to_path_buf(),
            app_handle,
            register_state,
            permissions,
            _epoch_owner: epoch_owner,
            driver: Mutex::new(None),
            http_servers: Arc::new(HttpServerRegistry::new()),
        })
    }

    pub(crate) fn relocate(&mut self, path: PathBuf) {
        self.plugin_root = path;
    }

    fn build_wasi_ctx(&self) -> Result<WasiCtx> {
        let mut builder = WasiCtxBuilder::new();
        builder.stdout(PluginStdioStreamV4::new(
            &self.name,
            crate::plugin::PluginStdioKind::Stdout,
        ));
        builder.stderr(PluginStdioStreamV4::new(
            &self.name,
            crate::plugin::PluginStdioKind::Stderr,
        ));
        builder
            .preopened_dir(&self.plugin_root, ".", FsPerms::ReadWrite)
            .map_err(anyhow::Error::from)
            .with_context(|| {
                format!(
                    "Failed to pre-open directory for plugin: {}",
                    self.plugin_root.display()
                )
            })?;
        Ok(builder.build())
    }

    fn build_linker(&self) -> Result<Linker<PluginCtxV4>> {
        let mut linker = Linker::new(&self.engine);

        // p2 与 p3 都装上：即使插件本身是 p3 世界，它的语言运行时（wasi-libc、
        // SpiderMonkey、CPython 等）很可能仍然按 p2 链接标准库，两边都提供才不会
        // 在实例化时报缺少导入。
        wasmtime_wasi_v4::p2::add_to_linker_async(&mut linker)
            .map_err(anyhow::Error::from)
            .context("Failed to register WASI p2 with the level 4 linker")?;
        wasmtime_wasi_v4::p3::add_to_linker(&mut linker)
            .map_err(anyhow::Error::from)
            .context("Failed to register WASI p3 with the level 4 linker")?;
        wasmtime_wasi_http_v4::p2::add_only_http_to_linker_async(&mut linker)
            .map_err(anyhow::Error::from)
            .context("Failed to register wasi-http p2 with the level 4 linker")?;
        wasmtime_wasi_http_v4::p3::add_to_linker(&mut linker)
            .map_err(anyhow::Error::from)
            .context("Failed to register wasi-http p3 with the level 4 linker")?;

        PsysWorldV4::add_to_linker::<PluginCtxV4, PluginCtxV4>(&mut linker, |ctx| ctx)
            .map_err(anyhow::Error::from)
            .context("Failed to register the level 4 plugin host interface")?;

        Ok(linker)
    }

    fn create_store(
        &self,
        generation: u64,
        http_dispatch_tx: mpsc::UnboundedSender<HttpDispatch>,
    ) -> Result<Store<PluginCtxV4>> {
        let wasi_ctx = self.build_wasi_ctx()?;
        let mut store = Store::new(
            &self.engine,
            PluginCtxV4::new(
                wasi_ctx,
                self.app_handle.clone(),
                self.plugin_root.clone(),
                self.name.clone(),
                Arc::clone(&self.register_state),
                Arc::clone(&self.permissions),
                generation,
                Arc::clone(&self.http_servers),
                http_dispatch_tx,
            ),
        );
        store.limiter(|ctx| ctx.store_limits());
        // driver 是常驻的，超时不能 trap，否则一个慢插件跑够 ticks 就把自己弄死了。
        // 改成让出给事件循环并续上新的额度：跑飞的 guest 会被反复打断，但其它任务
        // 仍有机会推进。
        store.epoch_deadline_callback(|_| Ok(UpdateDeadline::Yield(PLUGIN_EPOCH_TICKS_PER_CALL)));
        store.set_epoch_deadline(PLUGIN_EPOCH_TICKS_PER_CALL);
        Ok(store)
    }

    /// 启动插件：实例化 + on-load，然后把 driver 挂起来常驻。
    pub(crate) async fn run(&self, generation: u64) -> Result<()> {
        self.stop().await;

        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<V4Command>();
        let (http_tx, mut http_rx) = mpsc::unbounded_channel::<HttpDispatch>();
        let mut store = self.create_store(generation, http_tx)?;
        let linker = self.build_linker()?;
        let component = self.component.clone();
        let plugin_name = self.name.clone();

        let (ready_tx, ready_rx) = oneshot::channel::<Result<()>>();

        let task = v4_runtime().spawn(async move {
            // 手动走 pre + instantiate，是为了拿到 raw `Instance`：HTTP 处理导出要在
            // 它上面单独做一次**可失败**的查找，没实现的插件不能因此实例化失败。
            let instance = match linker.instantiate_pre(&component) {
                Ok(pre) => match pre.instantiate_async(&mut store).await {
                    Ok(instance) => instance,
                    Err(err) => {
                        let _ = ready_tx.send(Err(anyhow::anyhow!(
                            "Failed to instantiate plugin component for api_level=4. detail: {err}"
                        )));
                        return;
                    }
                },
                Err(err) => {
                    let _ = ready_tx.send(Err(anyhow::anyhow!(
                        "Failed to link plugin component for api_level=4. detail: {err}"
                    )));
                    return;
                }
            };

            let world = match PsysWorldV4::new(&mut store, &instance) {
                Ok(world) => world,
                Err(err) => {
                    let _ = ready_tx.send(Err(anyhow::anyhow!(
                        "Failed to load api level 4 world exports. detail: {err}"
                    )));
                    return;
                }
            };

            // 没导出就是 None，调用 http-server.start 时再报错给插件。
            let http_export = match PsysPluginHttpExport::new(&mut store, &instance) {
                Ok(export) => Some(export),
                Err(_) => {
                    log::debug!(
                        "[plugin:{plugin_name}] no http handler export; http-server is unavailable"
                    );
                    None
                }
            };

            let result = store
                .run_concurrent(async |accessor| {
                    // on-load 是 async func，可以直接 await 宿主接口，不必再 block_on。
                    if let Err(err) = world
                        .astrobox_psys_plugin_v4_lifecycle()
                        .call_on_load(accessor)
                        .await
                    {
                        let _ = ready_tx.send(Err(anyhow::anyhow!(
                            "Failed to execute the plugin on-load callback: {err}"
                        )));
                        return;
                    }
                    let _ = ready_tx.send(Ok(()));

                    let events = world.astrobox_psys_plugin_v4_event();
                    let http = http_export
                        .as_ref()
                        .map(|export| export.astrobox_psys_plugin_v4_http());
                    let mut in_flight = FuturesUnordered::new();

                    loop {
                        tokio::select! {
                            command = cmd_rx.recv() => {
                                let Some(command) = command else { break };
                                in_flight.push(Either::Left(
                                    dispatch(&plugin_name, events, accessor, command),
                                ));
                            }
                            request = http_rx.recv() => {
                                let Some(request) = request else { continue };
                                match http {
                                    Some(http) => in_flight.push(Either::Right(
                                        dispatch_http(&plugin_name, http, accessor, request),
                                    )),
                                    None => {
                                        let _ = request.reply.send(Err(
                                            "plugin does not export astrobox:psys-plugin-v4/http"
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                            // 只在有在途调用时才 poll，否则 FuturesUnordered 会立刻
                            // 返回 None 把这里变成忙等。
                            Some(()) = in_flight.next(), if !in_flight.is_empty() => {}
                        }
                    }

                    // 通道关闭说明插件正在停，等在途的调用收尾再退出。
                    while in_flight.next().await.is_some() {}
                })
                .await;

            if let Err(err) = result {
                log::error!("[plugin:{plugin_name}] api level 4 driver exited with error: {err}");
            }
        });

        match ready_rx.await {
            Ok(Ok(())) => {
                let mut guard = self.driver.lock().await;
                *guard = Some(DriverHandle { cmd_tx, task });
                Ok(())
            }
            Ok(Err(err)) => {
                task.abort();
                Err(err)
            }
            Err(_) => {
                task.abort();
                Err(anyhow::anyhow!(
                    "api level 4 driver for plugin '{}' exited before reporting readiness",
                    self.name
                ))
            }
        }
    }

    pub(crate) async fn stop(&self) {
        // 先停监听，避免插件都停了端口还开着、请求进来打到已经没了的 driver。
        self.http_servers.stop_all().await;

        let handle = {
            let mut guard = self.driver.lock().await;
            guard.take()
        };
        if let Some(handle) = handle {
            // 丢掉发送端让 driver 自己收尾，再等它结束；卡住就硬停。
            drop(handle.cmd_tx);
            let abort_handle = handle.task.abort_handle();
            if tokio::time::timeout(std::time::Duration::from_secs(5), handle.task)
                .await
                .is_err()
            {
                log::warn!(
                    "[plugin:{}] api level 4 driver did not stop in time; aborting",
                    self.name
                );
                abort_handle.abort();
            }
        }
    }

    pub(crate) async fn is_running(&self) -> bool {
        self.driver.lock().await.is_some()
    }

    pub(crate) async fn send(&self, command: V4Command) -> Result<()> {
        let guard = self.driver.lock().await;
        let Some(handle) = guard.as_ref() else {
            return Err(anyhow::anyhow!(
                "Plugin '{}' instance is not initialized",
                self.name
            ));
        };
        handle
            .cmd_tx
            .send(command)
            .map_err(|_| anyhow::anyhow!("Plugin '{}' driver is no longer running", self.name))
    }
}

/// 把一次 HTTP 请求交给插件的 handler，结果回给 hyper 侧。
async fn dispatch_http(
    plugin_name: &str,
    http: &crate::v4::bindings_http::exports::astrobox::psys_plugin_v4::http::Guest,
    accessor: &wasmtime::component::Accessor<PluginCtxV4>,
    dispatch: HttpDispatch,
) {
    let HttpDispatch {
        server_id,
        request,
        reply,
    } = dispatch;
    let result = http
        .call_handle(accessor, server_id, request)
        .await
        .map_err(|err| {
            log::error!("[plugin:{plugin_name}] http handler failed: {err}");
            err.to_string()
        });
    let _ = reply.send(result);
}

/// 执行一条命令；错误只记日志，不能让 driver 退出。
async fn dispatch(
    plugin_name: &str,
    events: &plugin_event::Guest,
    accessor: &wasmtime::component::Accessor<PluginCtxV4>,
    command: V4Command,
) {
    let result = match command {
        V4Command::Event {
            event_type,
            payload,
        } => events
            .call_on_event(accessor, event_type, payload)
            .await
            .map(|_| ()),
        V4Command::UiEvent {
            event_id,
            event,
            payload,
        } => events
            .call_on_ui_event(accessor, event_id, event, payload)
            .await
            .map(|_| ()),
        V4Command::UiRender { element_id } => {
            events.call_on_ui_render(accessor, element_id).await
        }
        V4Command::CardRender { card_id } => events.call_on_card_render(accessor, card_id).await,
    };

    if let Err(err) = result {
        log::error!("[plugin:{plugin_name}] api level 4 callback failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用真实的 Level 4 插件组件校验宿主契约。
    ///
    /// `instantiate_pre` 会把组件声明的**全部导入**拿去和 linker 里注册的做类型匹配，
    /// 因此这条断言同时覆盖了：v4 的 15 个宿主接口签名是否与 WIT 一致、WASI p2/p3
    /// 是否都装齐（Rust guest 的 std 仍按 p2 链接）、以及 `async func` 的类型是否对得上。
    ///
    /// 需要先构建被测组件，默认跳过：
    ///   cargo build --release --target wasm32-wasip2   # 在插件工程里
    ///   ASTROBOX_V4_TEST_COMPONENT=/path/to/plugin.wasm \
    ///     cargo test -p pluginsystem --manifest-path src-tauri/Cargo.toml \
    ///     v4::runtime::tests::level4_component_satisfies_host_contract -- --ignored --nocapture
    #[test]
    #[ignore = "needs a prebuilt level 4 component; set ASTROBOX_V4_TEST_COMPONENT"]
    fn level4_component_satisfies_host_contract() {
        let Ok(path) = std::env::var("ASTROBOX_V4_TEST_COMPONENT") else {
            panic!("set ASTROBOX_V4_TEST_COMPONENT to a built level 4 plugin component");
        };
        let bytes = fs::read(&path).expect("read component");

        let engine = create_engine().expect("engine");
        let component = Component::new(&engine, &bytes).expect("compile component");

        let mut linker: Linker<PluginCtxV4> = Linker::new(&engine);
        wasmtime_wasi_v4::p2::add_to_linker_async(&mut linker).expect("wasi p2");
        wasmtime_wasi_v4::p3::add_to_linker(&mut linker).expect("wasi p3");
        wasmtime_wasi_http_v4::p2::add_only_http_to_linker_async(&mut linker).expect("http p2");
        wasmtime_wasi_http_v4::p3::add_to_linker(&mut linker).expect("http p3");
        PsysWorldV4::add_to_linker::<PluginCtxV4, PluginCtxV4>(&mut linker, |ctx| ctx)
            .expect("astrobox host interfaces");

        // 这一步就是契约校验：缺任何一个导入、或签名对不上都会在这里报错。
        linker
            .instantiate_pre(&component)
            .expect("level 4 component imports must be fully satisfied by the host");

        // 若被测组件用的是 `psys-world-v4-http`，顺带确认 HTTP 处理导出真的在，
        // 也就是宿主查找用的名字和 WIT 对得上。
        let exports: Vec<_> = component
            .component_type()
            .exports(&engine)
            .map(|(name, _)| name.to_string())
            .collect();
        if std::env::var("ASTROBOX_V4_TEST_EXPECT_HTTP").is_ok() {
            assert!(
                exports
                    .iter()
                    .any(|name| name.starts_with("astrobox:psys-plugin-v4/http")),
                "component should export astrobox:psys-plugin-v4/http, got: {exports:?}"
            );
        }
    }
}
