//! Level 4 插件的 WebSocket 客户端。
//!
//! 连接由宿主建立并持有，插件只拿到一个 id：
//!
//!  * TCP / TLS 由宿主接（插件没有 `wasi:sockets`），`wss://` 用与应用其它地方一致的
//!    rustls + webpki 根证书；
//!  * 生命周期绑在插件实例上，插件停止或热重载时 `close_all` 全部关掉；
//!  * 收到的消息按插件在 `connect` 时选的方式送达：推送走 driver 的事件通道，**一条处理完
//!    才送下一条**；拉取放进有字节上限的缓冲，由 `receive` 取走。两种方式下插件跟不上时读任务
//!    都会停下来不读，压力留在 TCP 那头，一个吵闹的服务器撑不爆宿主内存。
//!
//! 每个连接一个读任务，它独占读半边；写半边放在锁里，插件的 `send`、保活 ping、`close`
//! 轮流用。读任务结束（对方关闭/断开）时把写半边一起丢掉，套接字随之释放。

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use base64::Engine as _;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use serde_json::json;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::{Request, Response};
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Bytes, Error as WsError, Message, Utf8Bytes};

use crate::v4::bindings::astrobox::psys_host_v4::websocket as bindings;

/// 每个插件同时开着的连接上限（含已被对方关闭、closed 还没被取走的）。
pub(crate) const MAX_CONNECTIONS: usize = 16;
/// 单条消息（收、发都算）的上限，与 HTTP 服务器的 body 上限一致。
const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
/// 拉取方式下最多替插件攒多少字节没取走的消息；攒满就先不读。
const PULL_BUFFER_BYTES: usize = 16 * 1024 * 1024;
/// 同上，按条数再兜一层底，免得几十万条空消息各占一点开销。
const PULL_BUFFER_MESSAGES: usize = 1024;
const DEFAULT_CONNECT_TIMEOUT_MS: u32 = 15_000;
const MIN_CONNECT_TIMEOUT_MS: u32 = 1_000;
const MAX_CONNECT_TIMEOUT_MS: u32 = 120_000;
#[cfg(not(test))]
const MIN_PING_INTERVAL_MS: u32 = 5_000;
// 测试里把保活周期压短，免得保活超时那条用例要跑十几秒。
#[cfg(test)]
const MIN_PING_INTERVAL_MS: u32 = 200;
/// 一条消息 30 秒还写不出去，基本可以认定对面不收了。
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// 插件 close 之后等对方回关闭帧的时间；到点就直接断开。
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
/// 插件停止时给每个连接发「going away」的时间。
const GOING_AWAY_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_URL_BYTES: usize = 8 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_PROTOCOLS: usize = 16;
const MAX_EVENT_NAME_BYTES: usize = 256;
/// 关闭帧载荷最多 125 字节，扣掉 2 字节关闭码。
const MAX_CLOSE_REASON_BYTES: usize = 123;

/// 握手要用、由宿主（tungstenite）负责的请求头，插件不能改。
/// 子协议走 `connect-options.protocols`，长度相关的头对 GET 握手没有意义还可能被拿来夹带。
const RESERVED_HEADERS: &[&str] = &[
    "host",
    "connection",
    "upgrade",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-extensions",
    "sec-websocket-accept",
    "sec-websocket-protocol",
    "content-length",
    "transfer-encoding",
];

type WsStream = WebSocketStream<NetStream>;
type WsSink = SplitSink<WsStream, Message>;

/// 推送方式下，一条消息从读任务送进 driver 的信封。driver 在插件的 `on-event` 返回后
/// 触发 `done`，读任务等到它才去读下一条——同一连接上的消息因此严格按序、不会并发处理。
pub(crate) struct WsDispatch {
    pub(crate) message: String,
    pub(crate) done: oneshot::Sender<()>,
}

// ── 底层流 ─────────────────────────────────────────────────────────

enum NetStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for NetStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            NetStream::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            NetStream::Tls(stream) => Pin::new(&mut **stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for NetStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            NetStream::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            NetStream::Tls(stream) => Pin::new(&mut **stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            NetStream::Plain(stream) => Pin::new(stream).poll_flush(cx),
            NetStream::Tls(stream) => Pin::new(&mut **stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            NetStream::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            NetStream::Tls(stream) => Pin::new(&mut **stream).poll_shutdown(cx),
        }
    }
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, String> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(Arc::clone(config));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|err| format!("failed to set up TLS: {err}"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    // WebSocket 握手只走 HTTP/1.1；不声明的话个别只开 h2 的入口会直接拒绝。
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::clone(CONFIG.get_or_init(|| Arc::new(config))))
}

// ── 连接参数 ───────────────────────────────────────────────────────

/// 校验过的 `connect-options`。校验全部在建连之前做完，非法参数不会产生任何网络请求。
struct ConnectPlan {
    request: Request,
    host: String,
    port: u16,
    tls: Option<ServerName<'static>>,
    event_name: Option<String>,
    timeout: Duration,
    ping_interval: Option<Duration>,
}

fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn plan_connection(options: bindings::ConnectOptions) -> Result<ConnectPlan, String> {
    let raw = options.url.trim();
    if raw.len() > MAX_URL_BYTES {
        return Err("url is too long".to_string());
    }
    let mut url = url::Url::parse(raw).map_err(|err| format!("invalid url: {err}"))?;
    let secure = match url.scheme() {
        "ws" => false,
        "wss" => true,
        other => {
            return Err(format!(
                "unsupported url scheme {other:?}; use ws:// or wss://"
            ));
        }
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(
            "credentials in the url are not supported; send an Authorization header instead"
                .to_string(),
        );
    }
    url.set_fragment(None);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "the url has no port".to_string())?;
    let (host, server_name) = match url.host() {
        Some(url::Host::Domain(domain)) => {
            let name = ServerName::try_from(domain.to_string())
                .map_err(|_| format!("invalid host name {domain:?}"))?;
            (domain.to_string(), name)
        }
        Some(url::Host::Ipv4(ip)) => (ip.to_string(), ServerName::from(IpAddr::V4(ip))),
        Some(url::Host::Ipv6(ip)) => (ip.to_string(), ServerName::from(IpAddr::V6(ip))),
        None => return Err("the url has no host".to_string()),
    };

    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|err| format!("invalid url: {err}"))?;

    if options.headers.len() > MAX_HEADERS {
        return Err(format!("too many headers (at most {MAX_HEADERS})"));
    }
    let mut header_bytes = 0usize;
    for header in &options.headers {
        let lower = header.name.trim().to_ascii_lowercase();
        header_bytes += lower.len() + header.value.len();
        if header_bytes > MAX_HEADER_BYTES {
            return Err("headers are too large".to_string());
        }
        if RESERVED_HEADERS.contains(&lower.as_str()) {
            return Err(format!(
                "header {:?} is managed by the host and cannot be set",
                header.name
            ));
        }
        let name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|_| format!("invalid header name {:?}", header.name))?;
        // HeaderValue 拒收 CR/LF 等控制字符，插件没法借头夹带额外的请求行。
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| format!("invalid value for header {:?}", header.name))?;
        request.headers_mut().append(name, value);
    }

    if options.protocols.len() > MAX_PROTOCOLS {
        return Err(format!("too many subprotocols (at most {MAX_PROTOCOLS})"));
    }
    if !options.protocols.is_empty() {
        for protocol in &options.protocols {
            if protocol.is_empty() || !protocol.bytes().all(is_tchar) {
                return Err(format!("invalid subprotocol {protocol:?}"));
            }
        }
        let value = HeaderValue::from_str(&options.protocols.join(", "))
            .map_err(|_| "invalid subprotocol list".to_string())?;
        request
            .headers_mut()
            .insert(HeaderName::from_static("sec-websocket-protocol"), value);
    }

    let event_name = options
        .event_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    if event_name
        .as_ref()
        .is_some_and(|name| name.len() > MAX_EVENT_NAME_BYTES)
    {
        return Err("event name is too long".to_string());
    }

    let timeout_ms = options
        .timeout_ms
        .unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)
        .clamp(MIN_CONNECT_TIMEOUT_MS, MAX_CONNECT_TIMEOUT_MS);
    let ping_interval = options
        .ping_interval_ms
        .filter(|ms| *ms > 0)
        .map(|ms| Duration::from_millis(u64::from(ms.max(MIN_PING_INTERVAL_MS))));

    Ok(ConnectPlan {
        request,
        host,
        port,
        tls: secure.then_some(server_name),
        event_name,
        timeout: Duration::from_millis(u64::from(timeout_ms)),
        ping_interval,
    })
}

async fn establish(
    request: Request,
    host: &str,
    port: u16,
    tls: Option<ServerName<'static>>,
) -> Result<(WsStream, Response), String> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|err| format!("failed to connect to {host}:{port}: {err}"))?;
    // 消息多是小包，攒包只会拖慢交互。
    let _ = tcp.set_nodelay(true);
    let stream = match tls {
        Some(server_name) => {
            let connector = TlsConnector::from(tls_config()?);
            let tls = connector
                .connect(server_name, tcp)
                .await
                .map_err(|err| format!("TLS handshake with {host} failed: {err}"))?;
            NetStream::Tls(Box::new(tls))
        }
        None => NetStream::Plain(tcp),
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    tokio_tungstenite::client_async_with_config(request, stream, Some(config))
        .await
        .map_err(describe_handshake_error)
}

fn describe_handshake_error(err: WsError) -> String {
    match err {
        WsError::Http(response) => {
            let status = response.status();
            let body = response
                .body()
                .as_deref()
                .map(|body| String::from_utf8_lossy(&body[..body.len().min(256)]).into_owned())
                .filter(|body| !body.trim().is_empty());
            match body {
                Some(body) => format!("handshake rejected: HTTP {status}: {}", body.trim()),
                None => format!("handshake rejected: HTTP {status}"),
            }
        }
        other => format!("handshake failed: {}", describe_ws_error(&other)),
    }
}

fn describe_ws_error(err: &WsError) -> String {
    match err {
        WsError::Capacity(err) => format!("message too large: {err}"),
        WsError::Io(err) => format!("network error: {err}"),
        WsError::ConnectionClosed | WsError::AlreadyClosed => "connection closed".to_string(),
        other => other.to_string(),
    }
}

fn is_sendable_close_code(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1013 | 3000..=4999)
}

fn close_info(frame: Option<Option<CloseFrame>>, error: Option<String>) -> bindings::CloseInfo {
    match frame {
        // 对方发起或回应了关闭握手：那就是正常关闭，之后 TCP 怎么断的不重要。
        Some(Some(frame)) => bindings::CloseInfo {
            code: u16::from(frame.code),
            reason: frame.reason.as_str().to_owned(),
            error: None,
        },
        Some(None) => bindings::CloseInfo {
            code: 1005,
            reason: String::new(),
            error: None,
        },
        None => bindings::CloseInfo {
            code: 1006,
            reason: String::new(),
            error: Some(
                error.unwrap_or_else(|| "connection closed without a closing handshake".into()),
            ),
        },
    }
}

/// 推送方式下送给插件的那条插件消息（外层由 `host_plugin_message` 包上 `host: true`）。
fn push_message(id: u32, event_name: &str, incoming: &bindings::Incoming) -> String {
    let payload = match incoming {
        bindings::Incoming::Text(text) => json!({ "id": id, "type": "text", "data": text }),
        bindings::Incoming::Binary(data) => json!({
            "id": id,
            "type": "binary",
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        }),
        bindings::Incoming::Closed(info) => json!({
            "id": id,
            "type": "closed",
            "code": info.code,
            "reason": info.reason,
            "error": info.error,
        }),
    };
    crate::api::host::event::host_plugin_message(event_name, &payload.to_string())
}

// ── 连接与登记表 ───────────────────────────────────────────────────

struct Shared {
    /// 插件已经 `close` 了这个连接（或插件在停）：不再往插件送任何东西。
    /// 用 watch 而不是普通标志，是为了能叫醒正等在 `receive` 里的调用。
    abandoned: watch::Sender<bool>,
    /// 读任务已经结束：对方关了或断了，不能再发。
    finished: AtomicBool,
}

impl Shared {
    fn abandon(&self) {
        self.abandoned.send_replace(true);
    }

    fn is_abandoned(&self) -> bool {
        *self.abandoned.borrow()
    }
}

struct Queued {
    incoming: bindings::Incoming,
    /// 占着拉取缓冲的字节额度，消息被取走（或连同缓冲一起丢掉）时归还。
    _permit: Option<OwnedSemaphorePermit>,
}

enum Inbox {
    Push,
    Pull(Mutex<mpsc::Receiver<Queued>>),
}

struct Connection {
    /// 读任务结束时会被置空，套接字随之释放。
    sink: Arc<Mutex<Option<WsSink>>>,
    shared: Arc<Shared>,
    inbox: Inbox,
    reader: StdMutex<Option<JoinHandle<()>>>,
}

impl Connection {
    fn take_reader(&self) -> Option<JoinHandle<()>> {
        self.reader
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    }
}

/// 一个插件名下所有的 WebSocket 连接。和 HTTP 服务器的登记表一样跟着运行时走，
/// 插件停止或热重载时 `close_all`。
#[derive(Default)]
pub(crate) struct WebSocketRegistry {
    connections: StdMutex<HashMap<u32, Arc<Connection>>>,
    next_id: AtomicU32,
}

impl WebSocketRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn connections(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Arc<Connection>>> {
        self.connections
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn get(&self, id: u32) -> Option<Arc<Connection>> {
        self.connections().get(&id).cloned()
    }

    fn remove(&self, id: u32) -> Option<Arc<Connection>> {
        self.connections().remove(&id)
    }

    pub(crate) async fn connect(
        self: &Arc<Self>,
        plugin_name: String,
        options: bindings::ConnectOptions,
        push_tx: mpsc::UnboundedSender<WsDispatch>,
    ) -> Result<bindings::ConnectionInfo, String> {
        if self.connections().len() >= MAX_CONNECTIONS {
            return Err(format!(
                "too many websocket connections (at most {MAX_CONNECTIONS})"
            ));
        }
        let ConnectPlan {
            request,
            host,
            port,
            tls,
            event_name,
            timeout,
            ping_interval,
        } = plan_connection(options)?;

        let (ws, response) =
            match tokio::time::timeout(timeout, establish(request, &host, port, tls)).await {
                Ok(result) => result?,
                Err(_) => return Err(format!("connecting to {host}:{port} timed out")),
            };

        let protocol = response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value.to_str().ok().map(|value| bindings::Header {
                    name: name.as_str().to_string(),
                    value: value.to_string(),
                })
            })
            .collect();

        let (sink, stream) = ws.split();
        let sink = Arc::new(Mutex::new(Some(sink)));
        let (abandoned, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            abandoned,
            finished: AtomicBool::new(false),
        });
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let push = event_name.is_some();
        let (inbox, outlet) = match event_name {
            Some(event_name) => (
                Inbox::Push,
                Outlet::Push {
                    id,
                    event_name,
                    tx: push_tx,
                },
            ),
            None => {
                let (tx, rx) = mpsc::channel(PULL_BUFFER_MESSAGES);
                (
                    Inbox::Pull(Mutex::new(rx)),
                    Outlet::Pull {
                        tx,
                        budget: Arc::new(Semaphore::new(PULL_BUFFER_BYTES)),
                    },
                )
            }
        };

        // 读任务先建好但等放行：连接要先进登记表、句柄要先挂上，它才能开始送消息或自行注销，
        // 否则对方秒关时会出现「先注销、后登记」的死连接。下面直到放行都没有 await。
        let (start_tx, start_rx) = oneshot::channel();
        let reader = Reader {
            id,
            plugin: plugin_name.clone(),
            registry: Arc::downgrade(self),
            shared: Arc::clone(&shared),
            sink: Arc::clone(&sink),
            outlet,
            ping_interval,
        };
        let handle = tokio::spawn(reader.run(stream, start_rx));
        let connection = Arc::new(Connection {
            sink,
            shared,
            inbox,
            reader: StdMutex::new(Some(handle)),
        });
        {
            let mut connections = self.connections();
            if connections.len() >= MAX_CONNECTIONS {
                drop(connections);
                // 并发的 connect 抢先占满了：这条不登记，读任务没放行就会自己退出。
                if let Some(handle) = connection.take_reader() {
                    handle.abort();
                }
                return Err(format!(
                    "too many websocket connections (at most {MAX_CONNECTIONS})"
                ));
            }
            connections.insert(id, Arc::clone(&connection));
        }
        let _ = start_tx.send(());

        log::info!(
            "[plugin:{plugin_name}] websocket {id} connected to {host}:{port} ({})",
            if push { "push" } else { "pull" }
        );
        Ok(bindings::ConnectionInfo {
            id,
            protocol,
            headers,
        })
    }

    pub(crate) async fn send(&self, id: u32, message: bindings::Message) -> Result<(), String> {
        let connection = self
            .get(id)
            .ok_or_else(|| format!("no such websocket connection: {id}"))?;
        if connection.shared.finished.load(Ordering::Acquire) {
            return Err("connection closed".to_string());
        }
        let message = match message {
            bindings::Message::Text(text) => {
                if text.len() > MAX_MESSAGE_BYTES {
                    return Err(format!("message too large (at most {MAX_MESSAGE_BYTES} bytes)"));
                }
                Message::Text(Utf8Bytes::from(text))
            }
            bindings::Message::Binary(data) => {
                if data.len() > MAX_MESSAGE_BYTES {
                    return Err(format!("message too large (at most {MAX_MESSAGE_BYTES} bytes)"));
                }
                Message::Binary(Bytes::from(data))
            }
        };
        let result = tokio::time::timeout(SEND_TIMEOUT, async {
            let mut guard = connection.sink.lock().await;
            match guard.as_mut() {
                Some(sink) => sink
                    .send(message)
                    .await
                    .map_err(|err| describe_ws_error(&err)),
                None => Err("connection closed".to_string()),
            }
        })
        .await;
        match result {
            Ok(result) => result,
            Err(_) => Err("send timed out".to_string()),
        }
    }

    pub(crate) async fn receive(
        &self,
        id: u32,
        timeout_ms: Option<u64>,
    ) -> Result<Option<bindings::Incoming>, String> {
        let connection = self
            .get(id)
            .ok_or_else(|| format!("no such websocket connection: {id}"))?;
        let Inbox::Pull(inbox) = &connection.inbox else {
            return Err(
                "this connection delivers messages as plugin events; receive is unavailable"
                    .to_string(),
            );
        };
        let mut abandoned = connection.shared.abandoned.subscribe();
        // 同一连接上的多个 receive 依次排队，各取各的。
        let mut inbox = inbox.lock().await;
        if *abandoned.borrow_and_update() {
            return Err("connection closed".to_string());
        }
        let next = async {
            tokio::select! {
                next = inbox.recv() => Ok(next),
                // 等待期间插件把连接 close 了（或插件在停）：别让这次调用一直挂着。
                _ = abandoned.changed() => Err("connection closed".to_string()),
            }
        };
        let next = match timeout_ms {
            Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), next).await {
                Ok(next) => next?,
                Err(_) => return Ok(None),
            },
            None => next.await?,
        };
        match next {
            Some(Queued { incoming, .. }) => {
                if matches!(incoming, bindings::Incoming::Closed(_)) {
                    // closed 已经交到插件手里，这个 id 就此作废。
                    self.remove(id);
                }
                Ok(Some(incoming))
            }
            None => Err("connection closed".to_string()),
        }
    }

    pub(crate) async fn close(
        &self,
        id: u32,
        code: Option<u16>,
        reason: Option<String>,
    ) -> Result<(), String> {
        let code = code.unwrap_or(1000);
        if !is_sendable_close_code(code) {
            return Err(format!(
                "close code {code} cannot be sent; use 1000-1003, 1007-1013 or 3000-4999"
            ));
        }
        let reason = reason.unwrap_or_default();
        if reason.len() > MAX_CLOSE_REASON_BYTES {
            return Err(format!(
                "close reason too long (at most {MAX_CLOSE_REASON_BYTES} bytes)"
            ));
        }
        let connection = self
            .remove(id)
            .ok_or_else(|| format!("no such websocket connection: {id}"))?;
        connection.shared.abandon();
        let reader = connection.take_reader();

        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: Utf8Bytes::from(reason),
        };
        let sent = tokio::time::timeout(CLOSE_TIMEOUT, async {
            let mut guard = connection.sink.lock().await;
            match guard.as_mut() {
                Some(sink) => sink.send(Message::Close(Some(frame))).await.is_ok(),
                None => false,
            }
        })
        .await
        .unwrap_or(false);

        if let Some(reader) = reader {
            if sent {
                // 读任务接着读到对方回的关闭帧就会自己结束；对方不回就到点掐掉。
                tokio::spawn(async move {
                    let abort = reader.abort_handle();
                    if tokio::time::timeout(CLOSE_TIMEOUT, reader).await.is_err() {
                        abort.abort();
                    }
                });
            } else {
                reader.abort();
            }
        }
        Ok(())
    }

    /// 关掉插件名下的全部连接。插件停止或热重载时必须调用。
    pub(crate) fn close_all(&self) {
        let connections = std::mem::take(&mut *self.connections());
        for (_, connection) in connections {
            connection.shared.abandon();
            if let Some(reader) = connection.take_reader() {
                reader.abort();
            }
            // 礼貌地告诉对方「我走了」，但不让插件的停止等它。
            let sink = Arc::clone(&connection.sink);
            tokio::spawn(async move {
                let _ = tokio::time::timeout(GOING_AWAY_TIMEOUT, async move {
                    let mut guard = sink.lock().await;
                    if let Some(mut sink) = guard.take() {
                        let _ = sink
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Away,
                                reason: Utf8Bytes::from_static(""),
                            })))
                            .await;
                    }
                })
                .await;
            });
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.connections().len()
    }
}

// ── 读任务 ─────────────────────────────────────────────────────────

enum Outlet {
    Push {
        id: u32,
        event_name: String,
        tx: mpsc::UnboundedSender<WsDispatch>,
    },
    Pull {
        tx: mpsc::Sender<Queued>,
        budget: Arc<Semaphore>,
    },
}

impl Outlet {
    /// 把一条消息交给插件，等到插件处理完（推送）或放进缓冲（拉取）才返回。
    /// 返回 false 表示插件那头已经不在了。
    async fn deliver(&self, incoming: bindings::Incoming) -> bool {
        match self {
            Outlet::Push { id, event_name, tx } => {
                let message = push_message(*id, event_name, &incoming);
                let (done_tx, done_rx) = oneshot::channel();
                if tx
                    .send(WsDispatch {
                        message,
                        done: done_tx,
                    })
                    .is_err()
                {
                    return false;
                }
                done_rx.await.is_ok()
            }
            Outlet::Pull { tx, budget } => {
                let size = match &incoming {
                    bindings::Incoming::Text(text) => text.len(),
                    bindings::Incoming::Binary(data) => data.len(),
                    bindings::Incoming::Closed(_) => 0,
                };
                // closed 不占额度：缓冲满着也得能送进去。
                let permit = if matches!(incoming, bindings::Incoming::Closed(_)) {
                    None
                } else {
                    let cost = size.clamp(1, PULL_BUFFER_BYTES) as u32;
                    match Arc::clone(budget).acquire_many_owned(cost).await {
                        Ok(permit) => Some(permit),
                        Err(_) => return false,
                    }
                };
                tx.send(Queued {
                    incoming,
                    _permit: permit,
                })
                .await
                .is_ok()
            }
        }
    }
}

struct Reader {
    id: u32,
    plugin: String,
    registry: Weak<WebSocketRegistry>,
    shared: Arc<Shared>,
    sink: Arc<Mutex<Option<WsSink>>>,
    outlet: Outlet,
    ping_interval: Option<Duration>,
}

async fn next_tick(ticker: &mut Option<tokio::time::Interval>) {
    match ticker {
        Some(ticker) => {
            ticker.tick().await;
        }
        None => std::future::pending().await,
    }
}

impl Reader {
    async fn run(self, mut stream: SplitStream<WsStream>, start: oneshot::Receiver<()>) {
        if start.await.is_err() {
            return;
        }
        let id = self.id;
        let plugin = self.plugin.as_str();

        // Some(None) 是对方发了不带关闭码的关闭帧。
        let mut close_frame: Option<Option<CloseFrame>> = None;
        let mut error: Option<String> = None;
        let mut ticker = self.ping_interval.map(|period| {
            let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            // 插件处理消息期间不读也不 ping，错过的节拍顺延，不要回来一口气补发。
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            ticker
        });
        // 发出 ping 之后一直没收到任何东西；连续两个周期这样就当连接已死（手机切网、NAT 过期
        // 这类情况 TCP 自己要好几分钟才会报错）。只在等数据时计时，插件处理慢不算。
        let mut awaiting_reply = false;
        let mut silent_periods = 0u8;

        loop {
            let frame = tokio::select! {
                frame = stream.next() => frame,
                _ = next_tick(&mut ticker) => {
                    if awaiting_reply {
                        silent_periods += 1;
                        if silent_periods >= 2 {
                            error = Some("keepalive timed out".to_string());
                            break;
                        }
                    }
                    // 插件正在发东西就跳过这一拍，没必要和它抢锁。
                    let Ok(mut guard) = self.sink.try_lock() else { continue };
                    let Some(sink) = guard.as_mut() else { break };
                    match tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Ping(Bytes::new()))).await {
                        Ok(Ok(())) => awaiting_reply = true,
                        Ok(Err(err)) => {
                            error = Some(describe_ws_error(&err));
                            break;
                        }
                        Err(_) => {
                            error = Some("keepalive ping timed out".to_string());
                            break;
                        }
                    }
                    continue;
                }
            };

            let message = match frame {
                None => break,
                Some(Err(err)) => {
                    error = Some(describe_ws_error(&err));
                    break;
                }
                Some(Ok(message)) => message,
            };
            awaiting_reply = false;
            silent_periods = 0;
            let incoming = match message {
                Message::Text(text) => bindings::Incoming::Text(text.as_str().to_owned()),
                Message::Binary(data) => bindings::Incoming::Binary(data.to_vec()),
                // tungstenite 已经自动回了关闭帧；接着读，流会在握手完成后结束。
                Message::Close(frame) => {
                    close_frame = Some(frame);
                    continue;
                }
                // ping 由 tungstenite 自动回 pong；pong 只用来证明对方还活着。
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            };
            // 插件 close 之后只把关闭握手读完，不再往它那里送。
            if self.shared.is_abandoned() {
                continue;
            }
            if !self.outlet.deliver(incoming).await && !self.shared.is_abandoned() {
                // driver 已经停了：插件在停，`close_all` 马上会来收拾。
                return;
            }
        }

        self.shared.finished.store(true, Ordering::Release);
        if !self.shared.is_abandoned() {
            let info = close_info(close_frame, error);
            log::info!(
                "[plugin:{plugin}] websocket {id} closed: code={} reason={:?} error={:?}",
                info.code,
                info.reason,
                info.error
            );
            let delivered = self.outlet.deliver(bindings::Incoming::Closed(info)).await;
            // 推送方式下 closed 已经送到，id 就此作废；拉取方式等插件 receive 到 closed 再删。
            if !delivered || matches!(self.outlet, Outlet::Push { .. }) {
                if let Some(registry) = self.registry.upgrade() {
                    registry.remove(id);
                }
            }
        }
        // 写半边一起丢掉，套接字才会真正释放（读写两半共享同一个底层流）。
        self.sink.lock().await.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn options(url: &str) -> bindings::ConnectOptions {
        bindings::ConnectOptions {
            url: url.to_string(),
            headers: Vec::new(),
            protocols: Vec::new(),
            event_name: None,
            timeout_ms: None,
            ping_interval_ms: None,
        }
    }

    fn header(name: &str, value: &str) -> bindings::Header {
        bindings::Header {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    /// 一个只接一个连接的测试服务器，连接交给 `serve` 处理。
    async fn server<F, Fut>(serve: F) -> String
    where
        F: FnOnce(WebSocketStream<TcpStream>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve(ws).await;
        });
        format!("ws://{addr}/socket")
    }

    async fn echo(mut ws: WebSocketStream<TcpStream>) {
        while let Some(Ok(message)) = ws.next().await {
            match message {
                Message::Text(_) | Message::Binary(_) => {
                    if ws.send(message).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    }

    fn unused_push() -> mpsc::UnboundedSender<WsDispatch> {
        mpsc::unbounded_channel().0
    }

    #[test]
    fn rejects_bad_options_before_touching_the_network() {
        let cases: Vec<(bindings::ConnectOptions, &str)> = vec![
            (options("http://example.com"), "scheme"),
            (options("not a url"), "invalid url"),
            (options("wss://user:pass@example.com"), "credentials"),
            (
                bindings::ConnectOptions {
                    headers: vec![header("Sec-WebSocket-Key", "x")],
                    ..options("wss://example.com")
                },
                "managed by the host",
            ),
            (
                bindings::ConnectOptions {
                    headers: vec![header("Host", "evil.example")],
                    ..options("wss://example.com")
                },
                "managed by the host",
            ),
            (
                bindings::ConnectOptions {
                    headers: vec![header("X-Token", "a\r\nInjected: 1")],
                    ..options("wss://example.com")
                },
                "invalid value",
            ),
            (
                bindings::ConnectOptions {
                    headers: vec![header("bad name", "v")],
                    ..options("wss://example.com")
                },
                "invalid header name",
            ),
            (
                bindings::ConnectOptions {
                    protocols: vec!["chat, evil".to_string()],
                    ..options("wss://example.com")
                },
                "invalid subprotocol",
            ),
        ];
        for (options, expected) in cases {
            let url = options.url.clone();
            let Err(err) = plan_connection(options) else {
                panic!("{url} should have been rejected");
            };
            assert!(err.contains(expected), "{url}: {err}");
        }
    }

    #[test]
    fn plans_default_ports_tls_and_extra_headers() {
        let plan = plan_connection(bindings::ConnectOptions {
            headers: vec![header("Authorization", "Bearer t"), header("Origin", "https://a.b")],
            protocols: vec!["v2.chat".to_string(), "v1.chat".to_string()],
            event_name: Some("  ws  ".to_string()),
            timeout_ms: Some(10),
            ping_interval_ms: Some(10),
            ..options("wss://example.com/path?q=1#frag")
        })
        .unwrap();
        assert_eq!(plan.host, "example.com");
        assert_eq!(plan.port, 443);
        assert!(plan.tls.is_some());
        assert_eq!(plan.event_name.as_deref(), Some("ws"));
        assert_eq!(plan.timeout, Duration::from_millis(u64::from(MIN_CONNECT_TIMEOUT_MS)));
        assert_eq!(
            plan.ping_interval,
            Some(Duration::from_millis(u64::from(MIN_PING_INTERVAL_MS)))
        );
        assert_eq!(plan.request.uri().to_string(), "wss://example.com/path?q=1");
        let headers = plan.request.headers();
        assert_eq!(headers.get("authorization").unwrap(), "Bearer t");
        assert_eq!(headers.get("sec-websocket-protocol").unwrap(), "v2.chat, v1.chat");

        let plain = plan_connection(options("ws://[::1]:9000")).unwrap();
        assert_eq!((plain.host.as_str(), plain.port), ("::1", 9000));
        assert!(plain.tls.is_none());
        assert_eq!(plain.event_name, None);
        assert_eq!(plain.ping_interval, None);
    }

    #[test]
    fn close_codes_follow_rfc6455() {
        for code in [1000, 1001, 1003, 1007, 1013, 3000, 4999] {
            assert!(is_sendable_close_code(code), "{code}");
        }
        for code in [0, 999, 1004, 1005, 1006, 1014, 1015, 2999, 5000] {
            assert!(!is_sendable_close_code(code), "{code}");
        }
    }

    #[tokio::test]
    async fn pull_mode_round_trip_and_server_close() {
        let url = server(|mut ws| async move {
            let first = ws.next().await.unwrap().unwrap();
            ws.send(first).await.unwrap();
            let second = ws.next().await.unwrap().unwrap();
            ws.send(second).await.unwrap();
            ws.close(Some(CloseFrame {
                code: CloseCode::Library(4001),
                reason: Utf8Bytes::from_static("bye"),
            }))
            .await
            .unwrap();
            while ws.next().await.is_some() {}
        })
        .await;

        let registry = Arc::new(WebSocketRegistry::new());
        let info = registry
            .connect("t".into(), options(&url), unused_push())
            .await
            .unwrap();
        let id = info.id;

        // 还没发东西：超时返回 none 而不是报错。
        assert!(matches!(registry.receive(id, Some(50)).await, Ok(None)));

        registry
            .send(id, bindings::Message::Text("hello".into()))
            .await
            .unwrap();
        registry
            .send(id, bindings::Message::Binary(vec![0, 1, 2, 255]))
            .await
            .unwrap();
        let Ok(Some(bindings::Incoming::Text(text))) = registry.receive(id, Some(2000)).await
        else {
            panic!("expected the echoed text");
        };
        assert_eq!(text, "hello");
        let Ok(Some(bindings::Incoming::Binary(data))) = registry.receive(id, Some(2000)).await
        else {
            panic!("expected the echoed binary");
        };
        assert_eq!(data, [0, 1, 2, 255]);
        let Ok(Some(bindings::Incoming::Closed(info))) = registry.receive(id, Some(2000)).await
        else {
            panic!("expected closed");
        };
        assert_eq!((info.code, info.reason.as_str(), info.error), (4001, "bye", None));
        // closed 取走之后这个 id 作废。
        assert_eq!(registry.len(), 0);
        assert!(registry.receive(id, Some(10)).await.is_err());
        assert!(
            registry
                .send(id, bindings::Message::Text("x".into()))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn push_mode_delivers_in_order_and_waits_for_each_handler() {
        let url = server(|mut ws| async move {
            for index in 0..5 {
                ws.send(Message::Text(format!("m{index}").into())).await.unwrap();
            }
            ws.send(Message::Binary(Bytes::from_static(&[9, 8, 7])))
                .await
                .unwrap();
            ws.close(None).await.unwrap();
            while ws.next().await.is_some() {}
        })
        .await;

        let (push_tx, mut push_rx) = mpsc::unbounded_channel::<WsDispatch>();
        let registry = Arc::new(WebSocketRegistry::new());
        let info = registry
            .connect(
                "t".into(),
                bindings::ConnectOptions {
                    event_name: Some("ws.chat".into()),
                    ..options(&url)
                },
                push_tx,
            )
            .await
            .unwrap();
        // 推送方式不能 receive。
        assert!(registry.receive(info.id, Some(10)).await.is_err());

        let mut received = Vec::new();
        while let Some(dispatch) = push_rx.recv().await {
            // 上一条还没处理完时不应该有下一条在排队。
            assert!(push_rx.try_recv().is_err(), "messages must not overlap");
            let outer: serde_json::Value = serde_json::from_str(&dispatch.message).unwrap();
            assert_eq!(outer["eventName"], "ws.chat");
            assert_eq!(outer["host"], true);
            let inner: serde_json::Value =
                serde_json::from_str(outer["payload"].as_str().unwrap()).unwrap();
            assert_eq!(inner["id"], info.id);
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = dispatch.done.send(());
            let closed = inner["type"] == "closed";
            received.push(inner);
            if closed {
                break;
            }
        }
        let kinds: Vec<_> = received
            .iter()
            .map(|item| item["type"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            kinds,
            ["text", "text", "text", "text", "text", "binary", "closed"]
        );
        for (index, item) in received.iter().take(5).enumerate() {
            assert_eq!(item["data"], format!("m{index}"));
        }
        assert_eq!(received[5]["data"], "CQgH");
        assert_eq!(received[6]["code"], 1005);
        assert_eq!(received[6]["error"], serde_json::Value::Null);

        // 推送方式下 closed 送达后 id 作废。
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn plugin_close_sends_the_code_and_wakes_a_pending_receive() {
        let (seen_tx, seen_rx) = oneshot::channel();
        let url = server(|mut ws| async move {
            let mut close = None;
            while let Some(Ok(message)) = ws.next().await {
                if let Message::Close(frame) = message {
                    close = frame;
                }
            }
            let _ = seen_tx.send(close);
        })
        .await;

        let registry = Arc::new(WebSocketRegistry::new());
        let id = registry
            .connect("t".into(), options(&url), unused_push())
            .await
            .unwrap()
            .id;

        let waiting = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { registry.receive(id, None).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(registry.close(id, Some(1005), None).await.is_err());
        assert!(registry.close(id, None, Some("x".repeat(124))).await.is_err());
        registry
            .close(id, Some(4000), Some("done".into()))
            .await
            .unwrap();

        let pending = tokio::time::timeout(Duration::from_secs(2), waiting)
            .await
            .expect("receive must not hang after close")
            .unwrap();
        assert!(pending.is_err());

        let frame = tokio::time::timeout(Duration::from_secs(2), seen_rx)
            .await
            .unwrap()
            .unwrap()
            .expect("server should see a close frame");
        assert_eq!(u16::from(frame.code), 4000);
        assert_eq!(frame.reason.as_str(), "done");
        assert_eq!(registry.len(), 0);
        assert!(registry.close(id, None, None).await.is_err());
    }

    #[tokio::test]
    async fn abnormal_disconnect_is_reported_with_1006() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            // 不走关闭握手直接断开。
            drop(ws);
        });

        let registry = Arc::new(WebSocketRegistry::new());
        let id = registry
            .connect("t".into(), options(&format!("ws://{addr}")), unused_push())
            .await
            .unwrap()
            .id;
        let Ok(Some(bindings::Incoming::Closed(info))) = registry.receive(id, Some(2000)).await
        else {
            panic!("expected closed");
        };
        assert_eq!(info.code, 1006);
        assert!(info.error.is_some());
    }

    #[tokio::test]
    async fn handshake_rejection_reports_the_status() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 9\r\nconnection: close\r\n\r\nbad token",
                )
                .await;
        });
        let registry = Arc::new(WebSocketRegistry::new());
        let err = registry
            .connect("t".into(), options(&format!("ws://{addr}")), unused_push())
            .await
            .unwrap_err();
        assert!(err.contains("401"), "{err}");
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn custom_headers_and_subprotocol_reach_the_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (seen_tx, seen_rx) = oneshot::channel();
        tokio::spawn(async move {
            use tokio_tungstenite::tungstenite::handshake::server::{
                ErrorResponse, Request as ServerRequest, Response as ServerResponse,
            };
            let (stream, _) = listener.accept().await.unwrap();
            let callback = move |request: &ServerRequest, mut response: ServerResponse| {
                let headers = request.headers();
                let _ = seen_tx.send((
                    headers
                        .get("authorization")
                        .map(|v| v.to_str().unwrap().to_string()),
                    headers
                        .get("sec-websocket-protocol")
                        .map(|v| v.to_str().unwrap().to_string()),
                ));
                response
                    .headers_mut()
                    .insert("sec-websocket-protocol", HeaderValue::from_static("v1"));
                Ok::<_, ErrorResponse>(response)
            };
            let ws = tokio_tungstenite::accept_hdr_async(stream, callback)
                .await
                .unwrap();
            echo(ws).await;
        });

        let registry = Arc::new(WebSocketRegistry::new());
        let info = registry
            .connect(
                "t".into(),
                bindings::ConnectOptions {
                    headers: vec![header("Authorization", "Bearer abc")],
                    protocols: vec!["v2".into(), "v1".into()],
                    ..options(&format!("ws://{addr}"))
                },
                unused_push(),
            )
            .await
            .unwrap();
        assert_eq!(info.protocol.as_deref(), Some("v1"));
        let (authorization, protocols) = seen_rx.await.unwrap();
        assert_eq!(authorization.as_deref(), Some("Bearer abc"));
        assert_eq!(protocols.as_deref(), Some("v2, v1"));
        registry.close(info.id, None, None).await.unwrap();
    }

    #[tokio::test]
    async fn oversized_incoming_message_ends_the_connection() {
        let url = server(|mut ws| async move {
            let _ = ws
                .send(Message::Binary(Bytes::from(vec![0u8; MAX_MESSAGE_BYTES + 1])))
                .await;
            while ws.next().await.is_some() {}
        })
        .await;
        let registry = Arc::new(WebSocketRegistry::new());
        let id = registry
            .connect("t".into(), options(&url), unused_push())
            .await
            .unwrap()
            .id;
        let Ok(Some(bindings::Incoming::Closed(info))) = registry.receive(id, Some(5000)).await
        else {
            panic!("expected closed");
        };
        assert_eq!(info.code, 1006);
        assert!(
            info.error.as_deref().unwrap_or_default().contains("too large"),
            "{info:?}"
        );
    }

    #[tokio::test]
    async fn connection_limit_and_close_all() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (closed_tx, mut closed_rx) = mpsc::unbounded_channel::<Option<CloseFrame>>();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else { break };
                let closed_tx = closed_tx.clone();
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    let mut frame = None;
                    while let Some(Ok(message)) = ws.next().await {
                        if let Message::Close(close) = message {
                            frame = close;
                        }
                    }
                    let _ = closed_tx.send(frame);
                });
            }
        });
        let url = format!("ws://{addr}");
        let registry = Arc::new(WebSocketRegistry::new());
        for _ in 0..MAX_CONNECTIONS {
            registry
                .connect("t".into(), options(&url), unused_push())
                .await
                .unwrap();
        }
        let err = registry
            .connect("t".into(), options(&url), unused_push())
            .await
            .unwrap_err();
        assert!(err.contains("too many"), "{err}");

        registry.close_all();
        assert_eq!(registry.len(), 0);
        for _ in 0..MAX_CONNECTIONS {
            let frame = tokio::time::timeout(Duration::from_secs(3), closed_rx.recv())
                .await
                .expect("every connection should be closed")
                .unwrap();
            assert_eq!(frame.map(|frame| u16::from(frame.code)), Some(1001));
        }
    }

    #[tokio::test]
    async fn keepalive_detects_a_silent_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            // 握手完就再也不读：ping 永远等不到回应。
            tokio::time::sleep(Duration::from_secs(3600)).await;
            drop(ws);
        });
        let registry = Arc::new(WebSocketRegistry::new());
        let plan_interval = Duration::from_millis(u64::from(MIN_PING_INTERVAL_MS));
        let id = registry
            .connect(
                "t".into(),
                bindings::ConnectOptions {
                    ping_interval_ms: Some(MIN_PING_INTERVAL_MS),
                    ..options(&format!("ws://{addr}"))
                },
                unused_push(),
            )
            .await
            .unwrap()
            .id;
        let started = tokio::time::Instant::now();
        let Ok(Some(bindings::Incoming::Closed(info))) =
            registry.receive(id, Some(5 * plan_interval.as_millis() as u64)).await
        else {
            panic!("expected keepalive to close the connection");
        };
        assert_eq!(info.code, 1006);
        assert_eq!(info.error.as_deref(), Some("keepalive timed out"));
        assert!(started.elapsed() >= plan_interval * 2);
    }

    /// 真实 `wss://` 端点的冒烟测试，验证 TLS（rustls + ring + webpki 根证书）整条链路。
    /// 需要外网，默认跳过：
    ///   cargo test -p pluginsystem websocket::tests::tls_echo -- --ignored
    #[tokio::test]
    #[ignore = "needs internet access"]
    async fn tls_echo() {
        let url = std::env::var("ASTROBOX_WS_TLS_ECHO")
            .unwrap_or_else(|_| "wss://echo.websocket.org".to_string());
        let registry = Arc::new(WebSocketRegistry::new());
        let id = registry
            .connect("t".into(), options(&url), unused_push())
            .await
            .expect("tls connect")
            .id;
        registry
            .send(id, bindings::Message::Text("astrobox-tls-check".into()))
            .await
            .unwrap();
        let mut echoed = false;
        for _ in 0..3 {
            if let Ok(Some(bindings::Incoming::Text(text))) =
                registry.receive(id, Some(5000)).await
            {
                if text == "astrobox-tls-check" {
                    echoed = true;
                    break;
                }
            }
        }
        assert!(echoed, "echo server should send the message back");
        registry.close(id, None, None).await.unwrap();
    }
}
