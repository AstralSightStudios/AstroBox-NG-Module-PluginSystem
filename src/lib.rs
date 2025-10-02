use std::{
    any::Any,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, mpsc},
    thread,
};

use anyhow::{Result, anyhow};
use wasmtime::{
    Config, Engine, Store, component::Component, component::Linker, component::bindgen,
};
use wasmtime_wasi::WasiCtxBuilder;

pub mod api;

bindgen!({
    world: "psys-world",
    path: "wit",
});

pub use api::host::PluginCtx;

#[derive(Clone)]
struct PluginHandle {
    sender: mpsc::Sender<PluginCommand>,
}

enum PluginCommand {
    LoadWasm {
        path: PathBuf,
        responder: mpsc::Sender<Result<()>>,
    },
    Invoke {
        action: Box<
            dyn FnOnce(&mut Store<PluginCtx>, &PsysWorld) -> Result<Box<dyn Any + Send>> + Send,
        >,
        responder: mpsc::Sender<Result<Box<dyn Any + Send>>>,
    },
}

fn registry() -> &'static Mutex<HashMap<String, PluginHandle>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, PluginHandle>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn create_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.async_support(true);
    Engine::new(&config).map_err(Into::into)
}

pub fn create_linker(engine: &Engine) -> Result<Linker<PluginCtx>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    PsysWorld::add_to_linker::<PluginCtx, PluginCtx>(&mut linker, |ctx| ctx)?;
    Ok(linker)
}

pub fn create_store(engine: &Engine, ctx: PluginCtx) -> Store<PluginCtx> {
    Store::new(engine, ctx)
}

pub fn load_wasi<F>(plugin_name: impl Into<String>, configure: F) -> Result<()>
where
    F: FnOnce(&mut WasiCtxBuilder) + Send + 'static,
{
    let name = plugin_name.into();

    {
        let map = registry()
            .lock()
            .map_err(|_| anyhow!("plugin registry poisoned"))?;
        if map.contains_key(&name) {
            return Err(anyhow!("plugin `{}` already initialized", name));
        }
    }

    let (command_tx, command_rx) = mpsc::channel();
    let (init_tx, init_rx) = mpsc::channel();

    let worker_name = name.clone();
    let init_tx_worker = init_tx.clone();
    let init_tx_error = init_tx.clone();
    thread::Builder::new()
        .name(format!("plugin-{}", worker_name))
        .spawn(move || {
            if let Err(err) = plugin_worker_main(worker_name, configure, command_rx, init_tx_worker)
            {
                let _ = init_tx_error.send(Err(err));
            }
        })
        .map_err(|err| anyhow!("failed to spawn plugin worker: {err}"))?;

    drop(init_tx);

    let init_result = init_rx
        .recv()
        .map_err(|_| anyhow!("plugin `{}` worker terminated during initialization", name))?;
    init_result?;

    let mut map = registry()
        .lock()
        .map_err(|_| anyhow!("plugin registry poisoned"))?;
    if map
        .insert(name.clone(), PluginHandle { sender: command_tx })
        .is_some()
    {
        return Err(anyhow!("plugin `{}` already initialized", name));
    }

    Ok(())
}

pub fn load_wasm(plugin_name: &str, wasm_path: impl AsRef<Path>) -> Result<()> {
    let handle = {
        let map = registry()
            .lock()
            .map_err(|_| anyhow!("plugin registry poisoned"))?;
        map.get(plugin_name)
            .cloned()
            .ok_or_else(|| anyhow!("plugin `{}` not found", plugin_name))?
    };

    let (resp_tx, resp_rx) = mpsc::channel();
    handle
        .sender
        .send(PluginCommand::LoadWasm {
            path: wasm_path.as_ref().to_path_buf(),
            responder: resp_tx,
        })
        .map_err(|_| anyhow!("plugin `{}` worker is unavailable", plugin_name))?;

    let response = resp_rx
        .recv()
        .map_err(|_| anyhow!("plugin `{}` worker disconnected", plugin_name))?;
    response
}

pub fn with_plugin<F, R>(plugin_name: &str, f: F) -> Result<R>
where
    F: FnOnce(&mut Store<PluginCtx>, &PsysWorld) -> Result<R> + Send + 'static,
    R: Send + 'static,
{
    let handle = {
        let map = registry()
            .lock()
            .map_err(|_| anyhow!("plugin registry poisoned"))?;
        map.get(plugin_name)
            .cloned()
            .ok_or_else(|| anyhow!("plugin `{}` not found", plugin_name))?
    };

    let (resp_tx, resp_rx) = mpsc::channel();
    handle
        .sender
        .send(PluginCommand::Invoke {
            action: Box::new(move |store, instance| {
                let result = f(store, instance)?;
                Ok(Box::new(result) as Box<dyn Any + Send>)
            }),
            responder: resp_tx,
        })
        .map_err(|_| anyhow!("plugin `{}` worker is unavailable", plugin_name))?;

    let response = resp_rx
        .recv()
        .map_err(|_| anyhow!("plugin `{}` worker disconnected", plugin_name))?;
    let boxed = response?;
    boxed
        .downcast::<R>()
        .map(|boxed| *boxed)
        .map_err(|_| anyhow!("plugin `{}` invocation result type mismatch", plugin_name))
}

fn plugin_worker_main<F>(
    plugin_name: String,
    configure: F,
    command_rx: mpsc::Receiver<PluginCommand>,
    init_tx: mpsc::Sender<Result<()>>,
) -> Result<()>
where
    F: FnOnce(&mut WasiCtxBuilder),
{
    let mut builder = WasiCtxBuilder::new();
    configure(&mut builder);
    let wasi_ctx = builder.build();

    let engine = create_engine()?;
    let linker = create_linker(&engine)?;
    let mut store = create_store(&engine, PluginCtx::new(wasi_ctx));

    let mut instance: Option<PsysWorld> = None;

    init_tx.send(Ok(())).map_err(|_| {
        anyhow!(
            "plugin `{}` initialization acknowledgement failed",
            plugin_name
        )
    })?;

    drop(init_tx);

    while let Ok(command) = command_rx.recv() {
        match command {
            PluginCommand::LoadWasm { path, responder } => {
                let result = (|| -> Result<()> {
                    let new_component = Component::from_file(&engine, &path)?;
                    let new_instance = PsysWorld::instantiate(&mut store, &new_component, &linker)?;
                    instance = Some(new_instance);
                    Ok(())
                })();
                let _ = responder.send(result);
            }
            PluginCommand::Invoke { action, responder } => {
                let result = (|| -> Result<Box<dyn Any + Send>> {
                    let instance_ref = instance.as_ref().ok_or_else(|| {
                        anyhow!("plugin `{}` has not loaded any component", plugin_name)
                    })?;
                    action(&mut store, instance_ref)
                })();
                let _ = responder.send(result);
            }
        }
    }

    Ok(())
}
