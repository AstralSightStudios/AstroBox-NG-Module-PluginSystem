use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

// Transport responses do not carry the host runtime generation. Keep a canceled
// request key quarantined for the request timeout so a late response cannot
// satisfy a request from the replacement plugin instance.
const RESPONSE_QUARANTINE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TransportMatchKey {
    device_addr: String,
    channel_id: u32,
    protobuf_type_id: Option<u32>,
    protobuf_packet_id: Option<u32>,
}

#[derive(Debug)]
pub struct TransportRequestWaiter {
    pub plugin_name: String,
    pub generation: u64,
    pub device_addr: String,
    pub channel_id: u32,
    pub protobuf_type_id: Option<u32>,
    pub protobuf_packet_id: Option<u32>,
    pub tx: oneshot::Sender<Vec<u8>>,
}

impl TransportRequestWaiter {
    fn match_key(&self) -> TransportMatchKey {
        TransportMatchKey {
            device_addr: self.device_addr.to_ascii_lowercase(),
            channel_id: self.channel_id,
            protobuf_type_id: self.protobuf_type_id,
            protobuf_packet_id: self.protobuf_packet_id,
        }
    }
}

#[derive(Debug)]
struct QuarantinedResponse {
    key: TransportMatchKey,
    expires_at: Instant,
}

#[derive(Default)]
struct TransportRuntimeState {
    waiters: Vec<TransportRequestWaiter>,
    quarantined: Vec<QuarantinedResponse>,
}

static TRANSPORT_RUNTIME_STATE: Lazy<Mutex<TransportRuntimeState>> =
    Lazy::new(|| Mutex::new(TransportRuntimeState::default()));

fn cleanup_quarantine(state: &mut TransportRuntimeState, now: Instant) {
    state.quarantined.retain(|entry| entry.expires_at > now);
}

pub(crate) fn register_request_waiter(
    plugin_name: &str,
    generation: u64,
    device_addr: String,
    channel_id: u32,
    protobuf_type_id: Option<u32>,
    protobuf_packet_id: Option<u32>,
) -> Option<oneshot::Receiver<Vec<u8>>> {
    let (tx, rx) = oneshot::channel();
    let waiter = TransportRequestWaiter {
        plugin_name: plugin_name.to_string(),
        generation,
        device_addr,
        channel_id,
        protobuf_type_id,
        protobuf_packet_id,
        tx,
    };
    let key = waiter.match_key();

    let mut state = TRANSPORT_RUNTIME_STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    cleanup_quarantine(&mut state, Instant::now());
    if state.quarantined.iter().any(|entry| entry.key == key) {
        log::debug!(
            "[pluginsystem] transport request rejected while response is quarantined: plugin={}, generation={}",
            plugin_name,
            generation
        );
        return None;
    }
    state.waiters.push(waiter);
    Some(rx)
}

pub(crate) fn cancel_request_waiters(plugin_name: &str, generation: u64) {
    let mut state = TRANSPORT_RUNTIME_STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let now = Instant::now();
    cleanup_quarantine(&mut state, now);
    let expires_at = now + RESPONSE_QUARANTINE;
    let waiters = std::mem::take(&mut state.waiters);
    let mut remaining = Vec::with_capacity(waiters.len());

    for waiter in waiters {
        if waiter.plugin_name == plugin_name && waiter.generation == generation {
            state.quarantined.push(QuarantinedResponse {
                key: waiter.match_key(),
                expires_at,
            });
        } else {
            remaining.push(waiter);
        }
    }
    state.waiters = remaining;
}

pub(crate) fn fulfill_request_waiters(
    device_addr: &str,
    channel_id: u32,
    protobuf_type_id: Option<u32>,
    protobuf_packet_id: Option<u32>,
    payload: &[u8],
) {
    let key = TransportMatchKey {
        device_addr: device_addr.to_ascii_lowercase(),
        channel_id,
        protobuf_type_id,
        protobuf_packet_id,
    };
    let mut state = TRANSPORT_RUNTIME_STATE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    cleanup_quarantine(&mut state, Instant::now());

    if let Some(index) = state.quarantined.iter().position(|entry| entry.key == key) {
        state.quarantined.swap_remove(index);
        log::debug!(
            "[pluginsystem] discarded quarantined transport response: addr={}, channel={}, packet_id={:?}",
            device_addr,
            channel_id,
            protobuf_packet_id
        );
        return;
    }

    let waiters = std::mem::take(&mut state.waiters);
    let mut remaining = Vec::with_capacity(waiters.len());
    for waiter in waiters {
        let waiter_key = waiter.match_key();
        if waiter_key == key {
            if waiter.tx.send(payload.to_vec()).is_err() {
                log::debug!("[pluginsystem] transport request waiter receiver dropped");
            }
        } else {
            remaining.push(waiter);
        }
    }

    state.waiters = remaining;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canceled_waiter_quarantines_a_late_response() {
        let device_addr = "test-generation-device".to_string();
        let plugin_name = "test-generation-plugin";

        let receiver =
            register_request_waiter(plugin_name, 1, device_addr.clone(), 2, Some(7), Some(9))
                .expect("initial waiter should be accepted");
        drop(receiver);
        cancel_request_waiters(plugin_name, 1);

        assert!(
            register_request_waiter(plugin_name, 2, device_addr.clone(), 2, Some(7), Some(9),)
                .is_none(),
            "new generation must not reuse a quarantined response key"
        );

        fulfill_request_waiters(&device_addr, 2, Some(7), Some(9), b"stale");

        let receiver =
            register_request_waiter(plugin_name, 2, device_addr.clone(), 2, Some(7), Some(9))
                .expect("key should be reusable after stale response is discarded");
        cancel_request_waiters(plugin_name, 2);
        fulfill_request_waiters(&device_addr, 2, Some(7), Some(9), b"cleanup");
        drop(receiver);
    }
}
