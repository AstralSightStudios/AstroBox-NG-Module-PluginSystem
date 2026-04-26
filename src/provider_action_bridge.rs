use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use tokio::sync::oneshot;

static NEXT_PROVIDER_ACTION_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static PENDING_PROVIDER_ACTIONS: OnceLock<Mutex<HashMap<String, oneshot::Sender<String>>>> =
    OnceLock::new();

fn pending_provider_actions() -> &'static Mutex<HashMap<String, oneshot::Sender<String>>> {
    PENDING_PROVIDER_ACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_pending_provider_action(
    provider: &str,
    action: &str,
) -> (String, oneshot::Receiver<String>) {
    let request_id = format!(
        "provider-action:{}:{}:{}",
        provider,
        action,
        NEXT_PROVIDER_ACTION_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );
    let (tx, rx) = oneshot::channel();
    pending_provider_actions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(request_id.clone(), tx);
    (request_id, rx)
}

pub fn resolve_pending_provider_action(request_id: &str, response: String) -> bool {
    let sender = pending_provider_actions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(request_id);

    match sender {
        Some(sender) => sender.send(response).is_ok(),
        None => false,
    }
}

pub fn cancel_pending_provider_action(request_id: &str) -> bool {
    pending_provider_actions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(request_id)
        .is_some()
}
