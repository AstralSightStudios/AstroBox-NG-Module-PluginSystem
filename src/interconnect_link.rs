//! 手环快应用 ⇄ 插件的 Interconnect 链路在宿主这一侧的维护。
//!
//! 以前这条链路只在「插件刚好已经按地址登记好、事件桥又没积压」时才通：
//!
//! * 消息在核心事件桥里**原地**投给插件，插件处理一条要等完一次 HTTP 请求；这期间事件桥和
//!   插件管理器的命令队列都卡着，广播通道一满（64 条）就整串丢掉，快应用的握手/请求再也
//!   等不到回应。
//! * 没有插件监听时消息直接扔掉。AB 刚启动、手环比插件先连上时，快应用一打开连接就发的
//!   握手正好落在这个空档里。
//! * 设备连上、断开从来没通知过插件（`DeviceAction` 一次都没派发过），插件只能在加载时登记；
//!   那时设备多半还没连上，插件只好登记空地址，而空地址又永远匹配不上。
//! * 手环问「手机端应用在不在」时一律答「在」，但手机端真正起来（插件登记好）之后从不主动
//!   推一次状态。官方 App 的做法是手机端应用连上、设备重连后主动推 SYNC_PHONE_APP_STATUS，
//!   手环据此重新打开快应用的连接，快应用会重新握手。
//!
//! 这些情况一旦撞上，就只能等手环重启、重连、重启 AB 之类随便哪个动作碰巧把顺序理顺。

use std::{
    collections::{HashMap, VecDeque},
    panic::AssertUnwindSafe,
    sync::OnceLock,
    time::{Duration, Instant},
};

use corelib::device::xiaomi::{XiaomiDevice, components::thirdparty_app::ThirdpartyAppSystem};
use futures_util::FutureExt;
use pb::xiaomi::protocol::phone_app_status::Status as PhoneAppStatus;
use tokio::sync::{Mutex, mpsc};

use crate::bindings::exports::astrobox::psys_plugin::event::EventType;

/// 没人接的消息留多久。快应用侧 fetch 默认 30 秒超时，补投必须赶在它放弃之前，
/// 也别把早就超时的请求再执行一遍。
const BACKLOG_TTL: Duration = Duration::from_secs(15);
const BACKLOG_CAPACITY: usize = 64;

struct Inbound {
    addr: String,
    pkg_name: String,
    payload: String,
    received_at: Instant,
}

static QUEUE: OnceLock<mpsc::UnboundedSender<Inbound>> = OnceLock::new();
static BACKLOG: OnceLock<Mutex<VecDeque<Inbound>>> = OnceLock::new();

fn backlog() -> &'static Mutex<VecDeque<Inbound>> {
    BACKLOG.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// 设备地址按 MAC 的写法宽松比较（大小写、`-`/`:` 分隔符）。
pub fn same_device_addr(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let normalize = |value: &str| value.trim().replace('-', ":").to_ascii_uppercase();
    normalize(left) == normalize(right)
}

/// 手环发来一条 Interconnect 消息。按到达顺序逐条投递，不阻塞调用方。
pub fn enqueue_interconnect_message(addr: String, pkg_name: String, payload: String) {
    push(Inbound {
        addr,
        pkg_name,
        payload,
        received_at: Instant::now(),
    });
}

fn push(message: Inbound) {
    let sender = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(consume(receiver));
        sender
    });
    if sender.send(message).is_err() {
        log::error!("[interconnect] delivery queue closed, message dropped");
    }
}

async fn consume(mut receiver: mpsc::UnboundedReceiver<Inbound>) {
    while let Some(message) = receiver.recv().await {
        // 这条循环一停，之后所有消息都会堆在队列里永远投不出去。
        if let Err(panic) = AssertUnwindSafe(deliver(message)).catch_unwind().await {
            log::error!(
                "[interconnect] delivery panicked: {}",
                crate::panic_payload_to_string(panic.as_ref())
            );
        }
    }
}

async fn deliver(message: Inbound) {
    let (receivers, plugin_runtime) = {
        // 查询监听者和放进暂存区要在同一把锁里，否则插件恰好在两者之间登记时，
        // 补投会扑空、消息又被放进暂存区，就再也没人来取了。
        let mut backlog = backlog().lock().await;
        let addr = message.addr.clone();
        let pkg_name = message.pkg_name.clone();
        let lookup = crate::with_plugin_manager_async(move |pm| {
            Box::pin(async move {
                (
                    pm.interconnect_receivers(&addr, &pkg_name).await,
                    tokio::runtime::Handle::current(),
                )
            })
        })
        .await;
        match lookup {
            Ok((receivers, handle)) if !receivers.is_empty() => (receivers, handle),
            Ok(_) | Err(_) => {
                // 插件系统还没起来，或者还没有插件监听这个快应用：先留着，插件一登记就补投。
                prune(&mut backlog);
                if backlog.len() >= BACKLOG_CAPACITY {
                    backlog.pop_front();
                }
                log::debug!(
                    "[interconnect] no listener for {} on {} yet, holding for {:?}",
                    message.pkg_name,
                    message.addr,
                    BACKLOG_TTL
                );
                backlog.push_back(message);
                return;
            }
        }
    };

    log::debug!(
        "[interconnect] dispatch addr={} pkg={} -> {} receiver(s)",
        message.addr,
        message.pkg_name,
        receivers.len()
    );

    // 仍旧放回插件运行时上执行（和以前在管理器命令里 spawn 的位置一致），每个插件一个任务，
    // 一个插件 panic 不会拖垮投递循环；等全部处理完再投下一条，保证同一快应用的帧按序到达。
    let handles = receivers
        .into_iter()
        .map(|(name, runtime)| {
            let payload = message.payload.clone();
            let task = plugin_runtime.spawn(async move {
                if let Err(err) = runtime.dispatch_interconnect_message(payload).await {
                    log::error!("[plugin:{name}] Failed to deliver interconnect message: {err}");
                }
            });
            task
        })
        .collect::<Vec<_>>();
    for handle in futures_util::future::join_all(handles).await {
        if let Err(err) = handle {
            log::error!("[interconnect] dispatch task panicked: {err}");
        }
    }
}

fn prune(backlog: &mut VecDeque<Inbound>) {
    backlog.retain(|message| message.received_at.elapsed() < BACKLOG_TTL);
}

/// 插件新登记了一条监听（重复登记不会走到这里）。
pub(crate) fn on_listener_added(addr: String, pkg_name: String) {
    // 登记发生在插件自己的宿主调用里，这里不能等任何东西（投递循环可能正等着这个插件）。
    tauri::async_runtime::spawn(async move {
        let replay = {
            let mut backlog = backlog().lock().await;
            prune(&mut backlog);
            let (hit, keep): (Vec<_>, Vec<_>) = backlog.drain(..).partition(|message| {
                message.pkg_name == pkg_name
                    && (addr.is_empty() || same_device_addr(&addr, &message.addr))
            });
            backlog.extend(keep);
            hit
        };
        if !replay.is_empty() {
            log::info!(
                "[interconnect] {} message(s) for {} arrived before anyone listened, delivering now",
                replay.len(),
                pkg_name
            );
            for message in replay {
                push(message);
            }
        }

        let targets = if addr.is_empty() {
            connected_xiaomi_devices().await
        } else {
            vec![addr]
        };
        for target in targets {
            sync_phone_app_connected(&target, &pkg_name).await;
        }
    });
}

/// 设备连上或断开。通知所有插件（`DeviceAction`），连上时再把监听着的快应用的
/// 「手机端应用已连接」推给手环——重连是新会话，手环不会记得上一次的状态。
pub fn notify_device_action(connected: bool, addr: String, name: Option<String>) {
    tauri::async_runtime::spawn(async move {
        let payload = serde_json::json!({
            "action": if connected { "connected" } else { "disconnected" },
            "addr": addr,
            "name": name,
        })
        .to_string();

        let lookup_addr = addr.clone();
        let lookup = crate::with_plugin_manager_async(move |pm| {
            Box::pin(async move {
                let packages = if connected {
                    pm.interconnect_packages_for(&lookup_addr).await
                } else {
                    Vec::new()
                };
                (
                    pm.active_runtimes(),
                    packages,
                    tokio::runtime::Handle::current(),
                )
            })
        })
        .await;
        let (runtimes, packages, plugin_runtime) = match lookup {
            Ok(result) => result,
            Err(err) => {
                log::debug!("[interconnect] device action not delivered: {err}");
                return;
            }
        };

        // 各插件各自一个任务：插件处理 DeviceAction 时可能要向手环查应用列表，不能互相等。
        for (name, runtime) in runtimes {
            let payload = payload.clone();
            plugin_runtime.spawn(async move {
                if let Err(err) = runtime.dispatch_event(EventType::DeviceAction, payload).await {
                    log::warn!("[plugin:{name}] Failed to deliver device action: {err}");
                }
            });
        }

        for pkg_name in packages {
            sync_phone_app_connected(&addr, &pkg_name).await;
        }
    });
}

async fn connected_xiaomi_devices() -> Vec<String> {
    corelib::ecs::with_rt_mut(|rt| {
        rt.device_ids()
            .filter(|device_id| rt.component_ref::<XiaomiDevice>(device_id.as_str()).is_some())
            .cloned()
            .collect::<Vec<_>>()
    })
    .await
}

/// 同一设备同一快应用短时间内只推一次：设备连上时按通配登记推过一次，插件收到
/// DeviceAction 后又按地址重新登记，会再触发一次。
const STATUS_SYNC_DEBOUNCE: Duration = Duration::from_secs(3);

fn claim_status_sync(addr: &str, pkg_name: &str) -> bool {
    static RECENT: OnceLock<std::sync::Mutex<HashMap<(String, String), Instant>>> =
        OnceLock::new();
    let mut recent = RECENT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    recent.retain(|_, at| at.elapsed() < STATUS_SYNC_DEBOUNCE);
    let key = (
        addr.trim().replace('-', ":").to_ascii_uppercase(),
        pkg_name.to_string(),
    );
    if recent.contains_key(&key) {
        return false;
    }
    recent.insert(key, Instant::now());
    true
}

async fn sync_phone_app_connected(addr: &str, pkg_name: &str) {
    if !claim_status_sync(addr, pkg_name) {
        return;
    }
    let app_info = match crate::api::host::thirdpartyapp::resolve_quick_app(addr, pkg_name).await
    {
        Ok(info) => info,
        Err(err) => {
            // 快应用没装在这台设备上，或设备不是小米的：手环本来也不会问。
            log::debug!("[interconnect] skip status sync for {pkg_name} on {addr}: {err}");
            return;
        }
    };
    let device_addr = addr.to_string();
    let result = corelib::ecs::with_rt_mut(move |rt| {
        rt.with_device_mut(&device_addr, |world, entity| {
            world
                .get_mut::<ThirdpartyAppSystem>(entity)
                .map(|mut system| system.sync_status(&app_info, PhoneAppStatus::Connected))
        })
        .flatten()
    })
    .await;
    match result {
        Some(Ok(())) => log::info!("[interconnect] told {addr} that the phone side of {pkg_name} is connected"),
        Some(Err(err)) => log::warn!("[interconnect] failed to sync status of {pkg_name} to {addr}: {err:#}"),
        None => {}
    }
}
