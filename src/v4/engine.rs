//! Level 4 的 wasmtime 引擎与 epoch 心跳。
//!
//! 与 `crate::plugin` 里 38.x 的那一套是**并行的两份**：epoch 注册表、心跳线程
//! 都必须按版本各开一份，因为 `Engine::increment_epoch` 属于各自版本的类型。

use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use wasmtime_v4 as wasmtime;

use wasmtime::{Config, Engine};

/// 单次 guest 调用允许消耗的 epoch 数。与 Level 2/3 保持一致。
pub(crate) const PLUGIN_EPOCH_TICKS_PER_CALL: u64 = 300;
const PLUGIN_EPOCH_TICK_INTERVAL: Duration = Duration::from_millis(100);

type EpochEngineEntry = (Engine, Weak<()>);

static EPOCH_ENGINE_REGISTRY: OnceLock<StdMutex<Vec<EpochEngineEntry>>> = OnceLock::new();
static EPOCH_TICKER_STARTED: OnceLock<()> = OnceLock::new();

fn epoch_engine_registry() -> &'static StdMutex<Vec<EpochEngineEntry>> {
    EPOCH_ENGINE_REGISTRY.get_or_init(|| StdMutex::new(Vec::new()))
}

/// 把引擎登记进 epoch 心跳；返回值的生命周期决定登记何时失效。
pub(crate) fn register_epoch_engine(engine: &Engine) -> Arc<()> {
    let owner = Arc::new(());
    epoch_engine_registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push((engine.clone(), Arc::downgrade(&owner)));

    EPOCH_TICKER_STARTED.get_or_init(|| {
        thread::spawn(|| {
            loop {
                thread::sleep(PLUGIN_EPOCH_TICK_INTERVAL);
                let mut entries = epoch_engine_registry()
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                entries.retain(|(engine, owner)| {
                    if owner.strong_count() == 0 {
                        false
                    } else {
                        engine.increment_epoch();
                        true
                    }
                });
            }
        });
    });

    owner
}

#[cfg(target_os = "ios")]
fn configure_platform(config: &mut Config) -> Result<()> {
    // iOS 不允许 JIT，走 Pulley 解释器；与 Level 2/3 的处理一致。
    let pulley_triple = if cfg!(target_pointer_width = "32") {
        if cfg!(target_endian = "big") {
            "pulley32be"
        } else {
            "pulley32"
        }
    } else if cfg!(target_endian = "big") {
        "pulley64be"
    } else {
        "pulley64"
    };

    // wasmtime 48 起不再复用 anyhow::Error，这里统一先转成 anyhow 再加上下文。
    config
        .target(pulley_triple)
        .map_err(anyhow::Error::from)
        .with_context(|| {
            format!(
                "failed to select Wasmtime interpreter target `{pulley_triple}` for iOS with moving memories"
            )
        })?;

    const RESERVE: u64 = 128 << 20; // 128 MiB

    config
        .memory_may_move(true)
        .memory_reservation(RESERVE)
        .memory_reservation_for_growth(RESERVE);

    log::info!(
        "[pluginsystem::v4] iOS detected; wasmtime 48 configured for interpreter mode via target `{pulley_triple}`"
    );

    Ok(())
}

#[cfg(not(target_os = "ios"))]
fn configure_platform(_config: &mut Config) -> Result<()> {
    Ok(())
}

/// 创建一个 Level 4 引擎（WASI p3 + component-model-async）。
pub(crate) fn create_engine() -> Result<Engine> {
    let mut config = Config::default();
    configure_platform(&mut config)?;
    // 48 起 async 支持恒定开启，`async_support` 已废弃且无效果，不要再设。
    config
        .wasm_memory64(false)
        .wasm_component_model(true)
        .wasm_component_model_async(true)
        .epoch_interruption(true);

    Engine::new(&config)
        .map_err(anyhow::Error::from)
        .context("Failed to initialize the Wasmtime 48 engine for API level 4")
}

#[cfg(test)]
mod tests {
    /// 双 wasmtime 共存的冒烟测试：两个 major 版本必须能在同一个进程里各自建引擎。
    #[test]
    fn both_wasmtime_versions_coexist() {
        let legacy = crate::plugin::create_engine_for_tests().expect("wasmtime 38 engine");
        let v4 = super::create_engine().expect("wasmtime 48 engine");

        // 两个版本各自的 epoch 心跳互不干扰。
        legacy.increment_epoch();
        v4.increment_epoch();
    }
}
