use anyhow::Error;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};
use tokio::sync::oneshot;
use wasmtime::component::{Accessor, FutureReader};

use crate::bindings::astrobox::psys_host;

use super::{HostString, HostVec, PluginCtx};

impl psys_host::dialog::Host for PluginCtx {}

impl psys_host::dialog::HostWithStore for PluginCtx {
    fn show_dialog<T>(
        accessor: &Accessor<T, Self>,
        dialog_type: psys_host::dialog::DialogType,
        style: psys_host::dialog::DialogStyle,
        info: psys_host::dialog::DialogInfo,
    ) -> impl core::future::Future<Output = FutureReader<psys_host::dialog::DialogResult>> + Send
    {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            let app_handle = {
                let ctx = access.get();
                ctx.app_handle()
            };
            FutureReader::new(instance, &mut access, async move {
                match (dialog_type, style) {
                    (
                        psys_host::dialog::DialogType::Alert,
                        psys_host::dialog::DialogStyle::System,
                    ) => show_system_alert(app_handle, info).await,
                    _ => {
                        log::warn!(
                            "dialog::show_dialog receive an unimplemented combination, type={:?} style={:?}, and return the default result",
                            dialog_type,
                            style
                        );
                        Ok(default_dialog_result())
                    }
                }
            })
        });
        async move { future }
    }

    fn pick_file<T>(
        accessor: &Accessor<T, Self>,
        config: psys_host::dialog::PickConfig,
        filter: psys_host::dialog::FilterConfig,
    ) -> impl core::future::Future<Output = FutureReader<psys_host::dialog::PickResult>> + Send
    {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = config;
                let _ = filter;
                Ok::<psys_host::dialog::PickResult, Error>(psys_host::dialog::PickResult {
                    name: HostString::default(),
                    data: HostVec::new(),
                })
            })
        });
        async move { future }
    }
}

async fn show_system_alert(
    app_handle: AppHandle,
    info: psys_host::dialog::DialogInfo,
) -> Result<psys_host::dialog::DialogResult, Error> {
    let title: String = info.title.into();
    let message: String = info.content.into();
    let mut buttons: Vec<ButtonSpec> = info.buttons.into_iter().map(ButtonSpec::from).collect();

    if buttons.is_empty() {
        log::debug!(
            "The system pop-up window does not provide a custom button, so use the default OK"
        );
    }

    buttons.sort_by(|a, b| b.primary.cmp(&a.primary));

    if buttons.len() > 3 {
        log::warn!(
            "The number of system pop-up buttons is {}, which exceeds the native limit and is truncated to the first three buttons",
            buttons.len()
        );
        buttons.truncate(3);
    }

    let (button_config, mapping) = build_button_config(buttons);
    let (tx, rx) = oneshot::channel();

    app_handle
        .dialog()
        .message(message)
        .title(title)
        .buttons(button_config)
        .show_with_result(move |result| {
            let clicked_btn_id = resolve_dialog_result(result, &mapping);
            let dialog_result = psys_host::dialog::DialogResult {
                clicked_btn_id,
                input_result: HostString::default(),
            };
            let _ = tx.send(dialog_result);
        });

    match rx.await {
        Ok(result) => Ok(result),
        Err(err) => {
            log::error!("Waiting for system pop-up result failed: {}", err);
            Ok(default_dialog_result())
        }
    }
}

fn build_button_config(buttons: Vec<ButtonSpec>) -> (MessageDialogButtons, Vec<ButtonSpec>) {
    if buttons.is_empty() {
        return (MessageDialogButtons::Ok, Vec::new());
    }

    let config = match buttons.len() {
        1 => MessageDialogButtons::OkCustom(buttons[0].label.clone()),
        2 => {
            MessageDialogButtons::OkCancelCustom(buttons[0].label.clone(), buttons[1].label.clone())
        }
        3 => MessageDialogButtons::YesNoCancelCustom(
            buttons[0].label.clone(),
            buttons[1].label.clone(),
            buttons[2].label.clone(),
        ),
        _ => MessageDialogButtons::Ok,
    };

    (config, buttons)
}

fn resolve_dialog_result(result: MessageDialogResult, buttons: &[ButtonSpec]) -> HostString {
    let clicked = match result {
        MessageDialogResult::Custom(label) => buttons
            .iter()
            .find(|b| b.label == label)
            .map(|b| b.id.clone()),
        MessageDialogResult::Ok | MessageDialogResult::Yes => buttons.first().map(|b| b.id.clone()),
        MessageDialogResult::No => buttons.get(1).map(|b| b.id.clone()),
        MessageDialogResult::Cancel => {
            if buttons.len() >= 3 {
                buttons.get(2).map(|b| b.id.clone())
            } else {
                buttons.last().map(|b| b.id.clone())
            }
        }
    }
    .unwrap_or_default();

    clicked.into()
}

fn default_dialog_result() -> psys_host::dialog::DialogResult {
    psys_host::dialog::DialogResult {
        clicked_btn_id: HostString::default(),
        input_result: HostString::default(),
    }
}

#[derive(Clone)]
struct ButtonSpec {
    id: String,
    label: String,
    primary: bool,
}

impl From<psys_host::dialog::DialogButton> for ButtonSpec {
    fn from(button: psys_host::dialog::DialogButton) -> Self {
        Self {
            id: button.id.into(),
            label: button.content.into(),
            primary: button.primary,
        }
    }
}
