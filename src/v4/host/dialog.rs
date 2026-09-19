//! Level 4 的 dialog 接口。
//!
//! v4 与 Level 2/3 的 dialog 记录在结构上完全一致，所以这里只做**类型搬运**，
//! 弹窗、文件选择、分片保存等全部逻辑仍然走 `crate::api::host::dialog` 里的那一份，
//! 不另起炉灶——否则两套实现迟早会在平台差异（父窗口、iOS 选择器等）上分叉。

use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use crate::api::host::dialog as shared;
use crate::bindings::astrobox::psys_host as psys_host_v2;
use crate::v4::bindings::astrobox::psys_host_v4::dialog;
use crate::v4::ctx::PluginCtxV4;

fn to_v2_dialog_type(value: dialog::DialogType) -> psys_host_v2::dialog::DialogType {
    match value {
        dialog::DialogType::Alert => psys_host_v2::dialog::DialogType::Alert,
        dialog::DialogType::Input => psys_host_v2::dialog::DialogType::Input,
    }
}

fn to_v2_dialog_info(info: dialog::DialogInfo) -> psys_host_v2::dialog::DialogInfo {
    psys_host_v2::dialog::DialogInfo {
        title: info.title.into(),
        content: info.content.into(),
        buttons: info
            .buttons
            .into_iter()
            .map(|button| psys_host_v2::dialog::DialogButton {
                id: button.id.into(),
                primary: button.primary,
                content: button.content.into(),
            })
            .collect(),
    }
}

fn to_v2_filter(filter: dialog::FilterConfig) -> psys_host_v2::dialog::FilterConfig {
    psys_host_v2::dialog::FilterConfig {
        multiple: filter.multiple,
        extensions: filter.extensions.into_iter().map(Into::into).collect(),
        default_directory: filter.default_directory.into(),
        default_file_name: filter.default_file_name.into(),
    }
}

fn to_v2_pick_config(config: dialog::PickConfig) -> psys_host_v2::dialog::PickConfig {
    psys_host_v2::dialog::PickConfig {
        read: config.read,
        copy_to: config.copy_to.map(Into::into),
    }
}

impl dialog::Host for PluginCtxV4 {
    fn open_url(&mut self, url: String) -> wasmtime::Result<()> {
        // open_url 在 Level 2/3 是 `&mut self` 方法且实现很短，这里直接复刻，
        // 免得为一个调用把 PluginCtx 也拖进来。
        let plugin_name = self.plugin_name().to_string();
        let app_handle = self.app_handle();
        match tauri_plugin_opener::OpenerExt::opener(&app_handle).open_url(url.clone(), None::<&str>)
        {
            Ok(()) => log::info!("[plugin:{}] dialog.open-url {}", plugin_name, url),
            Err(err) => log::warn!(
                "[plugin:{}] dialog.open-url failed for {}: {err}",
                plugin_name,
                url
            ),
        }
        Ok(())
    }
}

impl dialog::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn show_dialog(
        accessor: &Accessor<PluginCtxV4, Self>,
        dialog_type: dialog::DialogType,
        style: dialog::DialogStyle,
        info: dialog::DialogInfo,
    ) -> wasmtime::Result<dialog::DialogResult> {
        let (app_handle, plugin_name) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_name().to_string())
        });

        let v2_type = to_v2_dialog_type(dialog_type);
        let v2_info = to_v2_dialog_info(info);

        let result = match (dialog_type, style) {
            (dialog::DialogType::Alert, dialog::DialogStyle::System) => {
                shared::show_system_alert(app_handle, plugin_name, v2_info).await
            }
            (_, dialog::DialogStyle::Website) => {
                shared::show_website_dialog(app_handle, plugin_name, v2_type, v2_info).await
            }
            _ => {
                log::warn!(
                    "dialog::show_dialog receive an unimplemented combination, type={:?} style={:?}, and return the default result",
                    dialog_type,
                    style
                );
                Ok(shared::default_dialog_result())
            }
        }
        .map_err(wasmtime::Error::from_anyhow)?;

        Ok(dialog::DialogResult {
            clicked_btn_id: result.clicked_btn_id.into(),
            input_result: result.input_result.into(),
        })
    }

    async fn pick_file(
        accessor: &Accessor<PluginCtxV4, Self>,
        config: dialog::PickConfig,
        filter: dialog::FilterConfig,
    ) -> wasmtime::Result<Result<dialog::PickResult, String>> {
        let (app_handle, plugin_root) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_root().clone())
        });

        match shared::pick_file_with_dialog(
            app_handle,
            plugin_root,
            to_v2_pick_config(config),
            to_v2_filter(filter),
        )
        .await
        {
            Ok(result) => Ok(Ok(dialog::PickResult {
                name: result.name.into(),
                data: result.data.into_iter().collect(),
            })),
            Err(err) => Ok(Err(err.to_string())),
        }
    }

    async fn save_file_start(
        accessor: &Accessor<PluginCtxV4, Self>,
        filter: dialog::FilterConfig,
    ) -> wasmtime::Result<Result<dialog::SaveSession, String>> {
        let (app_handle, plugin_name) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_name().to_string())
        });

        match shared::save_file_start_with_dialog(app_handle, plugin_name, to_v2_filter(filter))
            .await
        {
            Ok(session) => Ok(Ok(dialog::SaveSession {
                session_id: session.session_id,
                name: session.name.into(),
            })),
            Err(()) => Ok(Err("save dialog cancelled or failed".to_string())),
        }
    }

    async fn save_file_write_chunk(
        accessor: &Accessor<PluginCtxV4, Self>,
        session_id: u64,
        data: Vec<u8>,
    ) -> wasmtime::Result<Result<(), String>> {
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        Ok(shared::save_file_write_chunk_impl(
            &plugin_name,
            session_id,
            &data,
        ))
    }

    async fn save_file_finish(
        accessor: &Accessor<PluginCtxV4, Self>,
        session_id: u64,
    ) -> wasmtime::Result<Result<(), String>> {
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        Ok(shared::save_file_finish_impl(&plugin_name, session_id))
    }

    async fn save_file_abort(
        accessor: &Accessor<PluginCtxV4, Self>,
        session_id: u64,
    ) -> wasmtime::Result<()> {
        let plugin_name = accessor.with(|mut access| access.get().plugin_name().to_string());
        shared::save_file_abort_impl(&plugin_name, session_id);
        Ok(())
    }
}
