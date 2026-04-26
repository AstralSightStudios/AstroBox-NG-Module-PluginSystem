use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::oneshot;

static NEXT_PROVIDER_ACTION_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
type ProgressReporter = Arc<dyn Fn(f32, String) + Send + Sync + 'static>;

struct PendingProviderAction {
    response_sender: oneshot::Sender<String>,
    progress_reporter: Option<ProgressReporter>,
}

static PENDING_PROVIDER_ACTIONS: OnceLock<Mutex<HashMap<String, PendingProviderAction>>> =
    OnceLock::new();

fn pending_provider_actions() -> &'static Mutex<HashMap<String, PendingProviderAction>> {
    PENDING_PROVIDER_ACTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_pending_provider_action(
    provider: &str,
    action: &str,
    progress_reporter: Option<ProgressReporter>,
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
        .insert(
            request_id.clone(),
            PendingProviderAction {
                response_sender: tx,
                progress_reporter,
            },
        );
    (request_id, rx)
}

pub fn resolve_pending_provider_action(request_id: &str, response: String) -> bool {
    let pending = pending_provider_actions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(request_id);

    match pending {
        Some(pending) => pending.response_sender.send(response).is_ok(),
        None => false,
    }
}

pub fn report_pending_provider_action_progress(
    request_id: &str,
    progress: f32,
    status: String,
) -> bool {
    let progress_reporter = pending_provider_actions()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(request_id)
        .and_then(|pending| pending.progress_reporter.as_ref().cloned());

    match progress_reporter {
        Some(progress_reporter) => {
            progress_reporter(progress, status);
            true
        }
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
