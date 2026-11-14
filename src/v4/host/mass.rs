//! Level 4 的 MASS 接口，以及把插件给的安装包交给宿主暂存的公共工具（`queue` 也用）。
//!
//! 插件的数据先落成文件再交给 core：core 的安装入口只收文件路径（大块数据不走 JSON），
//! 暂存目录选在前端/逻辑层本来就会清理的「插件下载目录」里，入队后的任务结束时由
//! 现成的善后命令删掉，不需要另造一套回收。

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use frontbridge::runtime::AppHandle;
use serde_json::{Value, json};
use tauri::Manager;
use tokio::sync::mpsc;
use wasmtime::component::Accessor;
use wasmtime_v4 as wasmtime;

use crate::api::host::permission::{check_permission_declared, resolve_device_name};
use crate::v4::bindings::astrobox::psys_host_v4::mass;
use crate::v4::ctx::PluginCtxV4;

/// 与 `app/logic` 里 `DOWNLOAD_DIR` 必须一致：只有直接放在这个目录下的文件，
/// 善后命令 `commprov_cleanup_plugin_provider_download` 才肯删。
pub(crate) const STAGING_DIR: &str = "plugin-provider-downloads";

/// 一次发送/入队的载荷上限。插件自己的内存上限是 256 MiB，bytes 走不到这么大；
/// 这个上限主要卡 file 变体（它不占插件内存）。
pub(crate) const MAX_PAYLOAD_BYTES: u64 = 1024 * 1024 * 1024;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub(crate) async fn staging_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|err| format!("cannot resolve the app cache directory: {err}"))?
        .join(STAGING_DIR);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| format!("cannot create the staging directory: {err}"))?;
    Ok(dir)
}

/// 扩展名只留短的字母数字，其余一律当没有；给暂存文件取名用，不参与任何判断。
fn safe_extension(name: &str) -> Option<String> {
    let extension = Path::new(name).extension()?.to_str()?;
    (!extension.is_empty() && extension.len() <= 8 && extension.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| extension.to_ascii_lowercase())
}

fn staged_name(extension: Option<&str>) -> String {
    let id = hex::encode(rand::random::<[u8; 16]>());
    match extension {
        Some(extension) => format!("plugin-{id}.{extension}"),
        None => format!("plugin-{id}"),
    }
}

/// 把内容写成暂存文件，返回路径。
pub(crate) async fn stage_bytes(app: &AppHandle, data: Vec<u8>) -> Result<PathBuf, String> {
    if data.is_empty() {
        return Err("the package is empty".to_string());
    }
    let path = staging_dir(app).await?.join(staged_name(Some("bin")));
    tokio::fs::write(&path, data)
        .await
        .map_err(|err| format!("cannot write the staged package: {err}"))?;
    Ok(path)
}

/// 插件视角的相对路径 → 宿主上的真实路径。必须落在插件目录内：不收绝对路径、`..`，
/// 也不认指到目录外的符号链接。
pub(crate) fn resolve_plugin_file(plugin_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = relative.trim();
    if relative.is_empty() {
        return Err("the file path is empty".to_string());
    }
    let candidate = Path::new(relative);
    let clean = candidate
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
    if !clean {
        return Err("the file path must be relative to the plugin directory".to_string());
    }
    let root = plugin_root
        .canonicalize()
        .map_err(|err| format!("cannot resolve the plugin directory: {err}"))?;
    let resolved = root
        .join(candidate)
        .canonicalize()
        .map_err(|err| format!("cannot open the file: {err}"))?;
    if !resolved.starts_with(&root) {
        return Err("the file path points outside the plugin directory".to_string());
    }
    if !resolved.is_file() {
        return Err("the path is not a file".to_string());
    }
    Ok(resolved)
}

/// 从插件目录移进暂存目录的文件：`staged` 是暂存后的位置，`origin` 是它在插件目录里的原位置。
pub(crate) struct StagedFile {
    pub(crate) staged: PathBuf,
    pub(crate) origin: PathBuf,
}

/// 把插件目录里的文件**移**进暂存目录（同盘是改名，不复制）。跨盘改名不了才退回复制再删。
pub(crate) async fn stage_file(
    app: &AppHandle,
    plugin_root: &Path,
    relative: &str,
) -> Result<StagedFile, String> {
    let source = resolve_plugin_file(plugin_root, relative)?;
    let size = tokio::fs::metadata(&source)
        .await
        .map_err(|err| format!("cannot read the file: {err}"))?
        .len();
    if size == 0 {
        return Err("the package is empty".to_string());
    }
    if size > MAX_PAYLOAD_BYTES {
        return Err("the package is too large".to_string());
    }
    let target = staging_dir(app).await?.join(staged_name(
        source.file_name().and_then(|name| name.to_str()).and_then(safe_extension).as_deref(),
    ));
    if tokio::fs::rename(&source, &target).await.is_ok() {
        return Ok(StagedFile { staged: target, origin: source });
    }
    if let Err(err) = tokio::fs::copy(&source, &target).await {
        // 复制到一半的残件不能留在暂存目录里
        remove_staged(&target).await;
        return Err(format!("cannot move the file into the staging directory: {err}"));
    }
    // 删不掉原件不影响这次安装：暂存副本已经是完整的
    if let Err(err) = tokio::fs::remove_file(&source).await {
        log::warn!("[pluginsystem] staged copy kept but the original could not be removed: {err}");
    }
    Ok(StagedFile { staged: target, origin: source })
}

/// 没能入队时把文件还给插件：放回它在插件目录里的原位置，插件改天重试不必重新下载。
///
/// 原位置上已经有了新文件、或者原目录不在插件目录里了（被换成了指向别处的链接），就不放回，
/// 直接删掉暂存件——宿主只往插件自己的目录里写。
pub(crate) async fn restore_staged(file: &StagedFile, plugin_root: &Path) {
    let restored = async {
        let file_name = file.origin.file_name()?;
        let root = plugin_root.canonicalize().ok()?;
        let parent = file.origin.parent()?.canonicalize().ok()?;
        if !parent.starts_with(&root) {
            return None;
        }
        let origin = parent.join(file_name);
        if tokio::fs::symlink_metadata(&origin).await.is_ok() {
            return None;
        }
        if tokio::fs::rename(&file.staged, &origin).await.is_ok() {
            return Some(());
        }
        // 跨盘：复制回去；复制失败就把残件清掉
        if tokio::fs::copy(&file.staged, &origin).await.is_ok() {
            return Some(());
        }
        let _ = tokio::fs::remove_file(&origin).await;
        None
    }
    .await;
    if restored.is_none() {
        log::warn!(
            "[pluginsystem] could not put {} back into the plugin directory; dropping the staged copy",
            file.origin.display()
        );
    }
    // 改名成功时暂存件已经不在了（NotFound 不报）；其余情况都要删掉
    remove_staged(&file.staged).await;
}

pub(crate) async fn remove_staged(path: &Path) {
    if let Err(err) = tokio::fs::remove_file(path).await {
        if err.kind() != std::io::ErrorKind::NotFound {
            log::warn!("[pluginsystem] failed to remove staged file {}: {err}", path.display());
        }
    }
}

fn data_type_label(data_type: &mass::DataType) -> &'static str {
    match data_type {
        mass::DataType::Watchface => "watchface",
        mass::DataType::Quickapp => "quickapp",
        mass::DataType::Firmware => "firmware",
        mass::DataType::NotificationIcon => "notification-icon",
    }
}

/// `MassDataType` 的数值（core 的 rpc 用数值）。
fn data_type_code(data_type: &mass::DataType) -> u8 {
    match data_type {
        mass::DataType::Watchface => 16,
        mass::DataType::Firmware => 32,
        mass::DataType::NotificationIcon => 50,
        mass::DataType::Quickapp => 64,
    }
}

/// 发送固件要单独的权限、且每次都问；其余类型共用 `mass`。
fn permission_for(data_type: &mass::DataType) -> &'static str {
    match data_type {
        mass::DataType::Firmware => "mass.firmware",
        _ => "mass",
    }
}

fn normalized_package_name(data_type: &mass::DataType, package_name: Option<String>) -> Result<String, String> {
    let name = package_name.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    match (data_type, name) {
        (_, Some(name)) => {
            if name.len() > 128 || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
                return Err("package-name contains invalid characters".to_string());
            }
            Ok(name)
        }
        (mass::DataType::NotificationIcon, None) => Err("package-name is required for notification-icon".to_string()),
        // 与前端安装快应用时一致的占位包名
        (_, None) => Ok("a.b.c".to_string()),
    }
}

impl mass::Host for PluginCtxV4 {}

impl mass::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn send(
        accessor: &Accessor<PluginCtxV4, Self>,
        device_addr: String,
        data_type: mass::DataType,
        data: Vec<u8>,
        options: mass::SendOptions,
    ) -> wasmtime::Result<Result<(), String>> {
        let (app_handle, plugin_name, permissions, generation) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
                ctx.runtime_generation(),
            )
        });

        let addr = device_addr.trim().to_string();
        if addr.is_empty() {
            return Ok(Err("device-addr is empty".to_string()));
        }
        if data.is_empty() {
            return Ok(Err("data is empty".to_string()));
        }
        let package_name = match normalized_package_name(&data_type, options.package_name) {
            Ok(name) => name,
            Err(err) => return Ok(Err(err)),
        };

        let params = json!({
            "plugin": plugin_name,
            "addr": addr,
            "deviceName": resolve_device_name(&addr).await,
            "dataType": data_type_label(&data_type),
            "size": data.len(),
            "packageName": package_name,
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), permission_for(&data_type), params)
            .await
        {
            return Ok(Err("permission denied".to_string()));
        }

        let progress_target = options
            .progress_event
            .map(|event| event.trim().to_string())
            .filter(|event| !event.is_empty())
            .map(|event| (plugin_name.clone(), generation, event));

        Ok(send_payload(
            &app_handle,
            addr,
            data_type_code(&data_type),
            data,
            package_name,
            options.watchface_id.map(|id| id.trim().to_string()).filter(|id| !id.is_empty()),
            progress_target,
        )
        .await)
    }
}

async fn ensure_xiaomi(addr: &str) -> Result<(), String> {
    let kind = corelib::rpc::call("device.kind", json!({ "addr": addr }), None)
        .await
        .map_err(|err| err.to_string())?;
    if kind.as_str() == Some("Xiaomi") {
        Ok(())
    } else {
        Err("MASS is only available on Xiaomi devices".to_string())
    }
}

async fn send_payload(
    app: &AppHandle,
    addr: String,
    res_type: u8,
    data: Vec<u8>,
    package_name: String,
    watchface_id: Option<String>,
    progress_target: Option<(String, u64, String)>,
) -> Result<(), String> {
    ensure_xiaomi(&addr).await?;
    let path = stage_bytes(app, data).await?;

    // 进度由 core 的同步回调给出，转成插件消息要走异步：开一个转发任务按序投递，
    // 避免每条进度各起一个任务导致乱序。
    let (progress, forwarder) = match progress_target {
        Some((plugin, generation, event)) => {
            let (tx, rx) = mpsc::unbounded_channel::<f64>();
            let task = tokio::spawn(forward_progress(plugin, generation, event, rx));
            (Some(tx), Some(task))
        }
        None => (None, None),
    };

    let callback: Option<corelib::rpc::Progress> = progress.clone().map(|tx| {
        let last = std::sync::Mutex::new(None::<Instant>);
        let callback: corelib::rpc::Progress = Arc::new(move |value: Value| {
            let Some(ratio) = value.get("progress").and_then(Value::as_f64) else {
                return;
            };
            let mut last = last.lock().unwrap_or_else(|poison| poison.into_inner());
            let due = last.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL);
            if due && ratio < 1.0 {
                *last = Some(Instant::now());
                let _ = tx.send(ratio.clamp(0.0, 1.0));
            }
        });
        callback
    });

    let result = corelib::rpc::call(
        "install.xiaomi",
        json!({
            "addr": addr,
            "resType": res_type,
            "path": path,
            "packageName": package_name,
            "watchfaceId": watchface_id,
        }),
        callback,
    )
    .await
    .map(|_| ())
    .map_err(|err| format!("{err:#}"));

    remove_staged(&path).await;
    if let Some(tx) = progress {
        if result.is_ok() {
            let _ = tx.send(1.0);
        }
        drop(tx);
    }
    if let Some(task) = forwarder {
        let _ = task.await;
    }
    result
}

async fn forward_progress(plugin: String, generation: u64, event: String, mut rx: mpsc::UnboundedReceiver<f64>) {
    while let Some(ratio) = rx.recv().await {
        let message = crate::api::host::event::host_plugin_message(
            &event,
            &json!({ "progress": ratio }).to_string(),
        );
        deliver_plugin_message(&plugin, generation, message).await;
    }
}

/// 把一条插件消息投给指定插件；插件已经停了/换代了就丢掉。
pub(crate) async fn deliver_plugin_message(plugin: &str, generation: u64, message: String) {
    let plugin = plugin.to_string();
    let log_name = plugin.clone();
    let result = crate::with_plugin_manager_async(move |pm| {
        Box::pin(async move {
            let Some(entry) = pm.plugins.get(&plugin) else {
                return;
            };
            if !entry.state.loaded || entry.state.disabled || !entry.runtime.is_generation_current(generation) {
                return;
            }
            if let Err(err) = entry.runtime.dispatch_plugin_message(message).await {
                log::warn!("[plugin:{plugin}] failed to deliver a plugin message: {err}");
            }
        })
    })
    .await;
    if let Err(err) = result {
        log::warn!("[plugin:{log_name}] failed to reach the plugin manager: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pluginsystem-mass-{name}-{}", hex::encode(rand::random::<[u8; 8]>())));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn plugin_files_must_stay_inside_the_plugin_directory() {
        let root = temp_root("resolve");
        std::fs::create_dir_all(root.join("cache")).unwrap();
        std::fs::write(root.join("cache/pkg.rpk"), b"data").unwrap();
        let outside = temp_root("outside");
        std::fs::write(outside.join("secret.bin"), b"secret").unwrap();

        let ok = resolve_plugin_file(&root, "cache/pkg.rpk").unwrap();
        assert!(ok.ends_with("cache/pkg.rpk"));
        assert!(resolve_plugin_file(&root, "./cache/pkg.rpk").is_ok());

        assert!(resolve_plugin_file(&root, "").is_err());
        assert!(resolve_plugin_file(&root, "../secret.bin").is_err());
        assert!(resolve_plugin_file(&root, "cache/../../secret.bin").is_err());
        assert!(resolve_plugin_file(&root, outside.join("secret.bin").to_str().unwrap()).is_err());
        assert!(resolve_plugin_file(&root, "cache").is_err(), "a directory is not a package");
        assert!(resolve_plugin_file(&root, "missing.bin").is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.join("secret.bin"), root.join("link.bin")).unwrap();
            assert!(resolve_plugin_file(&root, "link.bin").is_err(), "a symlink must not escape the plugin directory");
        }

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// 模拟一次「已移进暂存目录」：返回暂存记录（原位置用规范化后的绝对路径，与 stage_file 一致）。
    fn staged_from(root: &Path, relative: &str, staging: &Path) -> StagedFile {
        let origin = resolve_plugin_file(root, relative).unwrap();
        let staged = staging.join(staged_name(Some("bin")));
        std::fs::rename(&origin, &staged).unwrap();
        StagedFile { staged, origin }
    }

    #[tokio::test]
    async fn a_refused_plugin_file_goes_back_where_it_came_from() {
        let root = temp_root("restore");
        let staging = temp_root("restore-staging");
        std::fs::create_dir_all(root.join("downloads")).unwrap();
        std::fs::write(root.join("downloads/face.bin"), b"face").unwrap();

        let file = staged_from(&root, "downloads/face.bin", &staging);
        assert!(!root.join("downloads/face.bin").exists());
        restore_staged(&file, &root).await;
        assert_eq!(std::fs::read(root.join("downloads/face.bin")).unwrap(), b"face");
        assert!(!file.staged.exists(), "the staged copy is gone");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[tokio::test]
    async fn a_new_file_at_the_origin_is_not_overwritten() {
        let root = temp_root("restore-occupied");
        let staging = temp_root("restore-occupied-staging");
        std::fs::write(root.join("face.bin"), b"old").unwrap();

        let file = staged_from(&root, "face.bin", &staging);
        std::fs::write(root.join("face.bin"), b"new").unwrap();
        restore_staged(&file, &root).await;
        assert_eq!(std::fs::read(root.join("face.bin")).unwrap(), b"new");
        assert!(!file.staged.exists(), "the staged copy is dropped instead");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nothing_is_written_outside_the_plugin_directory() {
        let root = temp_root("restore-escape");
        let staging = temp_root("restore-escape-staging");
        let outside = temp_root("restore-escape-outside");
        std::fs::create_dir_all(root.join("downloads")).unwrap();
        std::fs::write(root.join("downloads/face.bin"), b"face").unwrap();

        let file = staged_from(&root, "downloads/face.bin", &staging);
        // 插件趁入队期间把原目录换成了指向外面的链接
        std::fs::remove_dir(root.join("downloads")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("downloads")).unwrap();
        restore_staged(&file, &root).await;
        assert!(!outside.join("face.bin").exists(), "must not follow the link out of the plugin directory");
        assert!(!file.staged.exists());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn staged_names_are_flat_and_extension_is_sanitised() {
        assert_eq!(safe_extension("pkg.RPK").as_deref(), Some("rpk"));
        assert_eq!(safe_extension("pkg").as_deref(), None);
        assert_eq!(safe_extension("pkg.tar.gz").as_deref(), Some("gz"));
        assert_eq!(safe_extension("pkg.a b").as_deref(), None);
        assert_eq!(safe_extension("pkg.waytoolongext").as_deref(), None);

        let name = staged_name(Some("rpk"));
        assert!(name.starts_with("plugin-") && name.ends_with(".rpk"));
        assert!(!name.contains('/') && !name.contains('\\'));
        assert_ne!(staged_name(None), staged_name(None));
    }

    #[test]
    fn package_names_are_validated() {
        let quick = mass::DataType::Quickapp;
        let icon = mass::DataType::NotificationIcon;
        assert_eq!(normalized_package_name(&quick, None).unwrap(), "a.b.c");
        assert_eq!(normalized_package_name(&quick, Some("  ".into())).unwrap(), "a.b.c");
        assert_eq!(normalized_package_name(&quick, Some("com.example.app".into())).unwrap(), "com.example.app");
        assert!(normalized_package_name(&icon, None).is_err());
        assert!(normalized_package_name(&quick, Some("bad name!".into())).is_err());
        assert!(normalized_package_name(&quick, Some("a".repeat(129))).is_err());
    }

    #[test]
    fn firmware_needs_its_own_permission() {
        assert_eq!(permission_for(&mass::DataType::Firmware), "mass.firmware");
        assert_eq!(permission_for(&mass::DataType::Watchface), "mass");
        assert_eq!(permission_for(&mass::DataType::Quickapp), "mass");
        assert_eq!(data_type_code(&mass::DataType::Watchface), 16);
        assert_eq!(data_type_code(&mass::DataType::Firmware), 32);
        assert_eq!(data_type_code(&mass::DataType::NotificationIcon), 50);
        assert_eq!(data_type_code(&mass::DataType::Quickapp), 64);
    }
}
