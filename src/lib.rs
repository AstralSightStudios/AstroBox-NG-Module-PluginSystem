use anyhow::{Result, anyhow};
use crossbeam_channel as channel;
use manager::PluginManager;
use once_cell::sync::OnceCell;
use std::{cell::RefCell, path::PathBuf, thread};
use tokio::sync::oneshot;

pub mod api;
pub mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "psys-world",
        imports: {
            "astrobox:psys-host/debug/send-raw": async | store,
            "astrobox:psys-host/device/disconnect-device": async | store,
            "astrobox:psys-host/interconnect/send-qaic-message": async | store,
            "astrobox:psys-host/thirdpartyapp/launch-qa": async | store,
            "astrobox:psys-host/thirdpartyapp/get-thirdparty-app-list": async | store,
        },
    });
}
pub mod manager;
pub mod manifest;
pub mod plugin;

thread_local! {
    static PM_IN_THREAD: RefCell<Option<*mut PluginManager>> = const { RefCell::new(None) };
}
static PLUGIN_THREAD_ID: OnceCell<thread::ThreadId> = OnceCell::new();

enum Command {
    Exec(Box<dyn FnOnce(&mut PluginManager) + Send + 'static>),
}
static PLUGIN_TX: OnceCell<channel::Sender<Command>> = OnceCell::new();

pub fn init(dir: PathBuf) -> Result<()> {
    let (tx, rx) = channel::unbounded::<Command>();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("Failed to build runtime: {e}");
                return;
            }
        };

        runtime.block_on(async move {
            let mut pm = PluginManager::new(dir);

            tokio::task::block_in_place(|| {
                PM_IN_THREAD.with(|cell| *cell.borrow_mut() = Some(&mut pm as *mut _));
                if PLUGIN_THREAD_ID.set(thread::current().id()).is_err() {
                    log::error!("PLUGIN_THREAD_ID 已设置");
                }
            });

            if let Err(e) = pm.load_from_dir() {
                log::error!("PluginManager init failed: {e}");
                return;
            }

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    Command::Exec(task) => {
                        task(&mut pm);
                    }
                }
            }
        });
    });

    PLUGIN_TX
        .set(tx)
        .map_err(|_| anyhow!("Plugin system already initialised"))
}

pub fn with_plugin_manager_sync<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut PluginManager) -> R,
{
    debug_assert_eq!(
        Some(thread::current().id()),
        PLUGIN_THREAD_ID.get().copied(),
        "with_plugin_manager_sync must be called from the plugin thread"
    );

    unsafe {
        PM_IN_THREAD.with(|cell| {
            let pm_ptr = cell
                .borrow()
                .ok_or_else(|| anyhow!("PluginManager TLS not set"))?
                as *mut PluginManager;
            Ok(f(&mut *pm_ptr))
        })
    }
}

pub async fn with_plugin_manager_async<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut PluginManager) -> R + Send + 'static,
    R: Send + 'static,
{
    if Some(thread::current().id()) == PLUGIN_THREAD_ID.get().copied() {
        return with_plugin_manager_sync(f);
    }

    let (tx, rx) = oneshot::channel();
    let cmd = Command::Exec(Box::new(move |pm| {
        let _ = tx.send(f(pm));
    }));

    PLUGIN_TX
        .get()
        .ok_or_else(|| anyhow!("Plugin system not initialised"))?
        .send(cmd)
        .map_err(|e| anyhow!("Plugin thread unexpectedly closed. error={}", e.to_string()))?;

    rx.await
        .map_err(|_| anyhow!("Plugin thread dropped the response"))
}
