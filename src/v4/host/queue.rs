//! Level 4 的下载队列接口。
//!
//! `add-resource-to-queue` 是老接口（本地文件走前端的「添加文件」流程）；
//! `enqueue-install` / `cancel-task` 是插件自建安装任务：安装包先交给宿主暂存，
//! 再由前端把它建成一个普通的队列任务——排队、进度、暂停、重试、断线恢复都和用户自己
//! 添加的任务一样，不另写一套。

use frontbridge::invoke_frontend;
use serde::Deserialize;
use serde_json::json;
use wasmtime::component::Accessor;
use wasmtime_v4 as wasmtime;

use crate::api::host::permission::{check_permission_declared, resolve_device_name};
use crate::v4::bindings::astrobox::psys_host_v4::queue;
use crate::v4::ctx::PluginCtxV4;
use crate::v4::host::mass::{
    MAX_PAYLOAD_BYTES, remove_staged, restore_staged, stage_bytes, stage_file,
};

const FRONT_FILE_ADD_TO_QUEUE_METHOD: &str = "host/file/add_to_queue";
const FRONT_ADD_PLUGIN_INSTALL_METHOD: &str = "host/queue/add_plugin_install";
const FRONT_CANCEL_PLUGIN_TASK_METHOD: &str = "host/queue/cancel_plugin_task";

const MAX_NAME_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 240;
const MAX_ICON_URL_CHARS: usize = 2048;
const MAX_ICON_DATA_URL_BYTES: usize = 256 * 1024;
const MAX_WATCHFACE_ID_CHARS: usize = 16;

fn resource_label(res_type: &queue::ResourceType) -> &'static str {
    match res_type {
        queue::ResourceType::Quickapp => "quickapp",
        queue::ResourceType::Watchface => "watchface",
        queue::ResourceType::Firmware => "firmware",
    }
}

impl queue::Host for PluginCtxV4 {
    fn add_resource_to_queue(
        &mut self,
        res_type: queue::ResourceType,
        file_path: String,
    ) -> wasmtime::Result<()> {
        let plugin_name = self.plugin_name().to_string();
        let app_handle = self.app_handle();
        let permissions = self.permissions();
        let params = json!({
            "plugin": plugin_name,
            "resourceType": resource_label(&res_type),
            "filePath": file_path,
        });

        tokio::spawn(async move {
            if !check_permission_declared(
                &app_handle,
                permissions.as_ref(),
                "queue",
                params,
            )
            .await
            {
                return;
            }

            let payload = json!({ "files": [file_path] });
            if let Err(err) = invoke_frontend::<bool, _>(
                &app_handle,
                FRONT_FILE_ADD_TO_QUEUE_METHOD,
                payload,
            )
            .await
            {
                log::warn!(
                    "[plugin:{}] failed to add resource to frontend queue: {}",
                    plugin_name,
                    err
                );
            }
        });

        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddedTask {
    task_id: String,
}

/// 名字、说明、图标都会原样显示在队列里：限长、去控制字符，免得一个插件把队列刷花。
fn clean_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

fn valid_icon(icon: &str) -> bool {
    if let Some(rest) = icon.strip_prefix("data:") {
        return rest.starts_with("image/") && icon.len() <= MAX_ICON_DATA_URL_BYTES;
    }
    icon.starts_with("https://") && icon.len() <= MAX_ICON_URL_CHARS && !icon.chars().any(char::is_whitespace)
}

fn valid_watchface_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= MAX_WATCHFACE_ID_CHARS && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// 入队参数的检查与清洗；返回清洗后的（名字、说明、图标、设备、表盘 ID）。
struct CleanTask {
    name: String,
    description: Option<String>,
    icon: Option<String>,
    device_addr: String,
    watchface_id: Option<String>,
}

fn clean_task(task: &queue::InstallTask) -> Result<CleanTask, String> {
    let name = clean_text(&task.name, MAX_NAME_CHARS);
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    let device_addr = task.device_addr.trim().to_string();
    if device_addr.is_empty() {
        return Err("device-addr is empty".to_string());
    }
    let description = task
        .description
        .as_deref()
        .map(|value| clean_text(value, MAX_DESCRIPTION_CHARS))
        .filter(|value| !value.is_empty());
    let icon = match task.icon.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(icon) if valid_icon(icon) => Some(icon.to_string()),
        Some(_) => return Err("icon must be an https URL or an image data URL".to_string()),
        None => None,
    };
    let watchface_id = match task.watchface_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(id) if matches!(task.res_type, queue::ResourceType::Watchface) && valid_watchface_id(id) => {
            Some(id.to_string())
        }
        Some(_) => return Err("watchface-id is only for watchfaces and must be 1-16 letters or digits".to_string()),
        None => None,
    };
    Ok(CleanTask {
        name,
        description,
        icon,
        device_addr,
        watchface_id,
    })
}

impl queue::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn enqueue_install(
        accessor: &Accessor<PluginCtxV4, Self>,
        task: queue::InstallTask,
    ) -> wasmtime::Result<Result<String, String>> {
        let (app_handle, plugin_name, plugin_root, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.plugin_root().clone(),
                ctx.permissions(),
            )
        });

        let clean = match clean_task(&task) {
            Ok(clean) => clean,
            Err(err) => return Ok(Err(err)),
        };

        let params = json!({
            "plugin": plugin_name,
            "name": clean.name,
            "resourceType": resource_label(&task.res_type),
            "addr": clean.device_addr,
            "deviceName": resolve_device_name(&clean.device_addr).await,
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), "queue", params).await {
            return Ok(Err("permission denied".to_string()));
        }

        // file 来源记下原位置：没能入队时要把文件还给插件
        let staged = match task.source {
            queue::InstallSource::File(relative) => stage_file(&app_handle, &plugin_root, &relative)
                .await
                .map(|file| (file.staged.clone(), Some(file))),
            queue::InstallSource::Bytes(bytes) => {
                if bytes.len() as u64 > MAX_PAYLOAD_BYTES {
                    Err("the package is too large".to_string())
                } else {
                    stage_bytes(&app_handle, bytes).await.map(|path| (path, None))
                }
            }
        };
        let (staged, from_plugin_file) = match staged {
            Ok(staged) => staged,
            Err(err) => return Ok(Err(err)),
        };

        let payload = json!({
            "plugin": plugin_name,
            "name": clean.name,
            "description": clean.description,
            "icon": clean.icon,
            "deviceAddr": clean.device_addr,
            "resourceType": resource_label(&task.res_type),
            "path": staged,
            "watchfaceId": clean.watchface_id,
        });
        match invoke_frontend::<AddedTask, _>(&app_handle, FRONT_ADD_PLUGIN_INSTALL_METHOD, payload).await {
            Ok(added) => Ok(Ok(added.task_id)),
            Err(err) => {
                // 没进队列就没人会清理这份暂存文件：插件给的文件放回原处，给的内容直接删掉
                match from_plugin_file {
                    Some(file) => restore_staged(&file, &plugin_root).await,
                    None => remove_staged(&staged).await,
                }
                log::warn!("[plugin:{plugin_name}] the frontend refused the install task: {err}");
                Ok(Err(err.to_string()))
            }
        }
    }

    async fn cancel_task(
        accessor: &Accessor<PluginCtxV4, Self>,
        task_id: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_name().to_string(), ctx.permissions())
        });
        // 只能动自己的任务，不需要再弹一次授权，但必须声明过 queue
        if !crate::api::host::permission::is_permission_declared(permissions.as_ref(), "queue") {
            return Ok(Err("permission denied".to_string()));
        }
        let task_id = task_id.trim().to_string();
        if task_id.is_empty() {
            return Ok(Err("task-id is empty".to_string()));
        }
        let payload = json!({ "plugin": plugin_name, "taskId": task_id });
        match invoke_frontend::<bool, _>(&app_handle, FRONT_CANCEL_PLUGIN_TASK_METHOD, payload).await {
            Ok(true) => Ok(Ok(())),
            Ok(false) => Ok(Err("no such task".to_string())),
            Err(err) => Ok(Err(err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(name: &str, res_type: queue::ResourceType) -> queue::InstallTask {
        queue::InstallTask {
            name: name.to_string(),
            description: None,
            icon: None,
            device_addr: " AA:BB ".to_string(),
            res_type,
            source: queue::InstallSource::Bytes(vec![1]),
            watchface_id: None,
        }
    }

    #[test]
    fn task_text_is_trimmed_and_limited() {
        let mut input = task("  Cool\nface\u{7} ", queue::ResourceType::Watchface);
        input.description = Some("x".repeat(500));
        let clean = clean_task(&input).unwrap();
        assert_eq!(clean.name, "Coolface");
        assert_eq!(clean.description.as_deref().map(|d| d.chars().count()), Some(MAX_DESCRIPTION_CHARS));
        assert_eq!(clean.device_addr, "AA:BB");

        assert!(clean_task(&task("   ", queue::ResourceType::Quickapp)).is_err());
        let mut no_device = task("ok", queue::ResourceType::Quickapp);
        no_device.device_addr = "  ".to_string();
        assert!(clean_task(&no_device).is_err());
    }

    #[test]
    fn icons_must_be_https_or_image_data_urls() {
        assert!(valid_icon("https://example.com/a.png"));
        assert!(valid_icon("data:image/png;base64,AAAA"));
        assert!(!valid_icon("http://example.com/a.png"));
        assert!(!valid_icon("file:///etc/passwd"));
        assert!(!valid_icon("javascript:alert(1)"));
        assert!(!valid_icon("data:text/html;base64,AAAA"));
        assert!(!valid_icon("https://example.com/a b.png"));
        assert!(!valid_icon(&format!("https://example.com/{}", "a".repeat(MAX_ICON_URL_CHARS))));

        let mut input = task("ok", queue::ResourceType::Quickapp);
        input.icon = Some("http://example.com/a.png".to_string());
        assert!(clean_task(&input).is_err());
        input.icon = Some("   ".to_string());
        assert!(clean_task(&input).unwrap().icon.is_none());
    }

    #[test]
    fn watchface_id_only_applies_to_watchfaces() {
        let mut input = task("ok", queue::ResourceType::Watchface);
        input.watchface_id = Some("1234567890".to_string());
        assert_eq!(clean_task(&input).unwrap().watchface_id.as_deref(), Some("1234567890"));
        input.watchface_id = Some("12345678901234567".to_string());
        assert!(clean_task(&input).is_err(), "longer than 16");
        input.watchface_id = Some("../x".to_string());
        assert!(clean_task(&input).is_err());

        let mut app = task("ok", queue::ResourceType::Quickapp);
        app.watchface_id = Some("1234".to_string());
        assert!(clean_task(&app).is_err());
    }
}
