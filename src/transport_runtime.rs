use once_cell::sync::Lazy;
use std::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Debug)]
pub struct TransportRequestWaiter {
    pub device_addr: String,
    pub channel_id: u32,
    pub protobuf_type_id: Option<u32>,
    pub protobuf_packet_id: Option<u32>,
    pub tx: oneshot::Sender<Vec<u8>>,
}

static TRANSPORT_REQUEST_WAITERS: Lazy<Mutex<Vec<TransportRequestWaiter>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

pub(crate) fn register_request_waiter(
    device_addr: String,
    channel_id: u32,
    protobuf_type_id: Option<u32>,
    protobuf_packet_id: Option<u32>,
) -> oneshot::Receiver<Vec<u8>> {
    let (tx, rx) = oneshot::channel();
    let waiter = TransportRequestWaiter {
        device_addr,
        channel_id,
        protobuf_type_id,
        protobuf_packet_id,
        tx,
    };

    let mut guard = TRANSPORT_REQUEST_WAITERS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    guard.push(waiter);
    rx
}

pub(crate) fn fulfill_request_waiters(
    device_addr: &str,
    channel_id: u32,
    protobuf_type_id: Option<u32>,
    protobuf_packet_id: Option<u32>,
    payload: &[u8],
) {
    let mut guard = TRANSPORT_REQUEST_WAITERS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut remaining = Vec::with_capacity(guard.len());

    for waiter in guard.drain(..) {
        let addr_match = waiter.device_addr.eq_ignore_ascii_case(device_addr);
        let channel_match = waiter.channel_id == channel_id;
        let protobuf_type_match = waiter.protobuf_type_id == protobuf_type_id;
        let protobuf_packet_match = waiter.protobuf_packet_id == protobuf_packet_id;

        if addr_match && channel_match && protobuf_type_match && protobuf_packet_match {
            if waiter.tx.send(payload.to_vec()).is_err() {
                log::debug!("[pluginsystem] transport request waiter receiver dropped");
            }
        } else {
            remaining.push(waiter);
        }
    }

    *guard = remaining;
}
