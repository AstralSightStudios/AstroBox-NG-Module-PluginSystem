use anyhow::Error;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath, MessageDialogButtons, MessageDialogResult};
use tauri_plugin_fs::FsExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use wasmtime::component::{Accessor, FutureReader};

use crate::bindings::astrobox::psys_host;

use super::{HostString, HostVec, PluginCtx};

impl psys_host::dialog::Host for PluginCtx {
    fn open_url(&mut self, url: HostString) -> wasmtime::Result<()> {
        let app_handle = self.app_handle();
        let url: String = url.into();
        if let Err(err) = app_handle.opener().open_url(url, None::<&str>) {
            log::warn!("Failed to open url in system browser: {err}");
        }
        Ok(())
    }
}

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
                    (_, psys_host::dialog::DialogStyle::Website) => {
                        show_website_dialog(app_handle, dialog_type, info).await
                    }
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
            let app_handle = {
                let ctx = access.get();
                ctx.app_handle()
            };
            let plugin_root = {
                let ctx = access.get();
                ctx.plugin_root().clone()
            };
            FutureReader::new(instance, &mut access, async move {
                pick_file_with_dialog(app_handle, plugin_root, config, filter).await
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

async fn show_website_dialog(
    app_handle: AppHandle,
    dialog_type: psys_host::dialog::DialogType,
    info: psys_host::dialog::DialogInfo,
) -> Result<psys_host::dialog::DialogResult, Error> {
    let payload = WebsiteDialogPayload::from(dialog_type, info);
    match frontbridge::invoke_frontend::<WebsiteDialogResult, _>(
        &app_handle,
        WEBSITE_DIALOG_METHOD,
        payload,
    )
    .await
    {
        Ok(resp) => Ok(psys_host::dialog::DialogResult {
            clicked_btn_id: resp.clicked_btn_id.into(),
            input_result: resp.input_result.into(),
        }),
        Err(err) => {
            log::warn!("dialog::show_dialog website fallback: {err}");
            Ok(default_dialog_result())
        }
    }
}

async fn pick_file_with_dialog(
    app_handle: AppHandle,
    plugin_root: std::path::PathBuf,
    config: psys_host::dialog::PickConfig,
    filter: psys_host::dialog::FilterConfig,
) -> Result<psys_host::dialog::PickResult, Error> {
    let mut builder = app_handle.dialog().file();
    let psys_host::dialog::FilterConfig {
        multiple,
        extensions,
        default_directory,
        default_file_name,
    } = filter;
    let default_dir: String = default_directory.into();
    if !default_dir.is_empty() {
        builder = builder.set_directory(default_dir);
    }
    let default_file_name: String = default_file_name.into();
    if !default_file_name.is_empty() {
        builder = builder.set_file_name(default_file_name);
    }

    let extensions: Vec<String> = extensions.into_iter().map(Into::into).collect();
    if !extensions.is_empty() {
        let exts: Vec<&str> = extensions.iter().map(String::as_str).collect();
        builder = builder.add_filter("files", &exts);
    }

    let (tx, rx) = oneshot::channel();
    if multiple {
        builder.pick_files(move |paths| {
            let _ = tx.send(paths);
        });
    } else {
        builder.pick_file(move |path| {
            let _ = tx.send(path.map(|item| vec![item]));
        });
    }

    let selected = match rx.await {
        Ok(Some(mut paths)) => {
            if multiple && paths.len() > 1 {
                log::warn!(
                    "dialog::pick_file requested multiple files, returning the first selection"
                );
            }
            paths.pop()
        }
        Ok(None) => None,
        Err(err) => {
            log::error!("dialog::pick_file waiting for selection failed: {err}");
            None
        }
    };

    let Some(file_path) = selected else {
        return Ok(psys_host::dialog::PickResult {
            name: HostString::default(),
            data: HostVec::new(),
        });
    };

    let file_name = resolve_file_name(&file_path);
    let should_read = config.read || config.copy_to.is_some();
    let file_data = if should_read {
        match app_handle.fs().read(file_path.clone()) {
            Ok(data) => data,
            Err(err) => {
                log::error!("dialog::pick_file failed to read file: {err}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    if let Some(target_dir) = config.copy_to {
        if let Some(dest) = build_copy_target(plugin_root, target_dir.into(), &file_name) {
            if let Some(parent) = dest.parent() {
                if let Err(err) = tokio::fs::create_dir_all(parent).await {
                    log::warn!("dialog::pick_file failed to create dir: {err}");
                }
            }
            if let Err(err) = tokio::fs::write(&dest, &file_data).await {
                log::warn!("dialog::pick_file failed to copy file: {err}");
            }
        }
    }

    let data = if config.read {
        file_data
    } else {
        Vec::new()
    };

    Ok(psys_host::dialog::PickResult {
        name: file_name.into(),
        data,
    })
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

fn resolve_file_name(path: &FilePath) -> String {
    match path {
        FilePath::Path(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        FilePath::Url(url) => url
            .path_segments()
            .and_then(|segments| segments.last())
            .map(|name| name.to_string())
            .unwrap_or_default(),
    }
}

fn build_copy_target(
    plugin_root: std::path::PathBuf,
    target: String,
    file_name: &str,
) -> Option<std::path::PathBuf> {
    let target_path = std::path::PathBuf::from(target);
    if target_path.is_absolute() {
        log::warn!("dialog::pick_file rejected absolute copy-to path");
        return None;
    }

    let mut safe_path = std::path::PathBuf::new();
    for component in target_path.components() {
        match component {
            std::path::Component::Normal(piece) => safe_path.push(piece),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                log::warn!("dialog::pick_file rejected copy-to with parent dir");
                return None;
            }
            _ => {
                log::warn!("dialog::pick_file rejected copy-to path component");
                return None;
            }
        }
    }

    let dest = if safe_path.extension().is_some() {
        safe_path
    } else {
        safe_path.join(file_name)
    };
    Some(plugin_root.join(dest))
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

const WEBSITE_DIALOG_METHOD: &str = "host/dialog/show_dialog";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebsiteDialogPayload {
    dialog_type: WebsiteDialogType,
    info: WebsiteDialogInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebsiteDialogInfo {
    title: String,
    content: String,
    buttons: Vec<WebsiteDialogButton>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebsiteDialogButton {
    id: String,
    primary: bool,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum WebsiteDialogType {
    Alert,
    Input,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebsiteDialogResult {
    clicked_btn_id: String,
    input_result: String,
}

impl WebsiteDialogPayload {
    fn from(dialog_type: psys_host::dialog::DialogType, info: psys_host::dialog::DialogInfo) -> Self {
        let dialog_type = match dialog_type {
            psys_host::dialog::DialogType::Alert => WebsiteDialogType::Alert,
            psys_host::dialog::DialogType::Input => WebsiteDialogType::Input,
        };
        Self {
            dialog_type,
            info: WebsiteDialogInfo::from(info),
        }
    }
}

impl From<psys_host::dialog::DialogInfo> for WebsiteDialogInfo {
    fn from(info: psys_host::dialog::DialogInfo) -> Self {
        Self {
            title: info.title.into(),
            content: info.content.into(),
            buttons: info.buttons.into_iter().map(WebsiteDialogButton::from).collect(),
        }
    }
}

impl From<psys_host::dialog::DialogButton> for WebsiteDialogButton {
    fn from(button: psys_host::dialog::DialogButton) -> Self {
        Self {
            id: button.id.into(),
            primary: button.primary,
            content: button.content.into(),
        }
    }
}
