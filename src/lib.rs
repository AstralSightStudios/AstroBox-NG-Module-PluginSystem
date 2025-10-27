use anyhow::{Error, Result};
use manager::PluginManager;
use once_cell::sync::OnceCell;
use std::future::Future;
use std::pin::Pin;
use std::{cell::RefCell, path::PathBuf, thread};
use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot};

pub mod api;
pub mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "psys-world",
        imports: {
            "astrobox:psys-host/os/arch": async | store,
            "astrobox:psys-host/os/hostname": async | store,
            "astrobox:psys-host/os/locale": async | store,
            "astrobox:psys-host/os/platform": async | store,
            "astrobox:psys-host/os/version": async | store,
            "astrobox:psys-host/transport/send": async | store,
            "astrobox:psys-host/transport/request": async | store,
            "astrobox:psys-host/ui/show-dialog": async | store,
            "astrobox:psys-host/device/get-device-list": async | store,
            "astrobox:psys-host/device/get-connected-device-list": async | store,
            "astrobox:psys-host/device/disconnect-device": async | store,
            "astrobox:psys-host/register/register-transport-recv": async | store,
            "astrobox:psys-host/register/register-interconnect-recv": async | store,
            "astrobox:psys-host/register/register-deeplink-action": async | store,
            "astrobox:psys-host/register/register-provider": async | store,
            "astrobox:psys-host/interconnect/send-qaic-message": async | store,
            "astrobox:psys-host/picker/pick-file": async | store,
            "astrobox:psys-host/thirdpartyapp/launch-qa": async | store,
            "astrobox:psys-host/thirdpartyapp/get-thirdparty-app-list": async | store,
        },
        exports: {
            default: async,
        },
    });
}
pub mod manager;
pub mod manifest;
pub mod plugin;

type CommandFuture<'pm> = Pin<Box<dyn Future<Output = ()> + 'pm>>;
enum Command {
    Exec(Box<dyn for<'pm> FnOnce(&'pm mut PluginManager) -> CommandFuture<'pm> + Send>),
}
static PLUGIN_TX: OnceCell<mpsc::UnboundedSender<Command>> = OnceCell::new();

thread_local! {
    static PM_IN_THREAD: RefCell<Option<*mut PluginManager>> = const { RefCell::new(None) };
}
static PLUGIN_THREAD_ID: OnceCell<thread::ThreadId> = OnceCell::new();

pub fn init(dir: PathBuf, app_handle: AppHandle) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Command>();

    std::thread::spawn(move || {
        log::info!("Building multi_thread plugin runtime...");
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

        let dir_cl = dir.clone();
        runtime.block_on(async move {
            let mut pm = PluginManager::new(dir, app_handle);

            tokio::task::block_in_place(|| {
                PM_IN_THREAD.with(|cell| *cell.borrow_mut() = Some(&mut pm as *mut _));
                if PLUGIN_THREAD_ID.set(thread::current().id()).is_err() {
                    log::error!("PLUGIN_THREAD_ID 已设置");
                }
            });

            log::info!(
                "Loading plugins from dir {}",
                dir_cl.to_string_lossy().to_string()
            );
            if let Err(e) = pm.load_from_dir().await {
                log::error!("PluginManager init failed: {e}");
                return;
            }

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Command::Exec(task) => {
                        task(&mut pm).await;
                    }
                }
            }
        });
    });

    PLUGIN_TX
        .set(tx)
        .map_err(|_| corelib::anyhow_site!("Plugin system already initialised"))
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
                .ok_or_else(|| corelib::anyhow_site!("PluginManager TLS not set"))?
                as *mut PluginManager;
            Ok(f(&mut *pm_ptr))
        })
    }
}

pub async fn with_plugin_manager_async<F, Fut, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut PluginManager) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    if Some(thread::current().id()) == PLUGIN_THREAD_ID.get().copied() {
        let fut = unsafe {
            PM_IN_THREAD.with(|cell| {
                let pm_ptr = cell
                    .borrow()
                    .ok_or_else(|| corelib::anyhow_site!("PluginManager TLS not set"))?
                    as *mut PluginManager;
                // Safety: 我们当前运行在插件线程内，pm_ptr 的独占访问得到保证
                Ok::<Fut, Error>(f(&mut *pm_ptr))
            })
        }?;
        return Ok(fut.await);
    }

    let (tx, rx) = oneshot::channel();
    let cmd = Command::Exec(Box::new(move |pm| {
        let fut = f(pm);
        Box::pin(async move {
            let result = fut.await;
            let _ = tx.send(result);
        })
    }));

    PLUGIN_TX
        .get()
        .ok_or_else(|| corelib::anyhow_site!("Plugin system not initialised"))?
        .send(cmd)
        .map_err(|e| corelib::anyhow_site!("Plugin thread unexpectedly closed. error={:?}", e))?;

    rx.await
        .map_err(|_| corelib::anyhow_site!("Plugin thread dropped the response"))
}
