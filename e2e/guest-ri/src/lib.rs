wit_bindgen::generate!({
    path: "../../wit",
    world: "psys-world-v4-resource-install",
    generate_all,
});

use astrobox::psys_host_v4::{mass, queue};
use exports::astrobox::psys_plugin_v4::resource_install::{Guest as ResourceInstall, InstallOutcome, InstallRequest};
use exports::astrobox::psys_plugin_v4::{event, lifecycle};

struct Plugin;
export!(Plugin);

impl lifecycle::Guest for Plugin {
    async fn on_load() {}
}

impl event::Guest for Plugin {
    async fn on_event(_t: event::EventType, _p: String) -> String { String::new() }
    async fn on_ui_event(_id: String, _e: astrobox::psys_host_v4::ui::Event, _p: String) -> String { String::new() }
    async fn on_ui_render(_id: String) {}
    async fn on_card_render(_id: String) {}
}

impl ResourceInstall for Plugin {
    async fn on_install(request: InstallRequest) -> InstallOutcome {
        // The manifest "data" drives what the test plugin does.
        let data = request.data.clone();
        match data.as_str() {
            "echo" => InstallOutcome::Failed(format!(
                "id={} provider={} res={}/{} type={} key={} ver={} code={:?} dev={} upd={} data={}",
                request.request_id, request.provider, request.resource_id, request.resource_name,
                request.resource_type, request.download_key, request.version, request.version_code,
                request.device_addr, request.is_update, request.data
            )),
            "queued" => InstallOutcome::Queued,
            "pass" => InstallOutcome::Pass,
            "redeem" => InstallOutcome::Redeem("请先输入兑换码".to_string()),
            "redeem-empty" => InstallOutcome::Redeem(String::new()),
            "fail" => InstallOutcome::Failed("boom 失败".to_string()),
            "enqueue-file" | "enqueue-bytes" => {
                let source = if data == "enqueue-file" {
                    queue::InstallSource::File("downloads/face.bin".to_string())
                } else {
                    queue::InstallSource::Bytes(vec![1, 2, 3, 250, 0, 255])
                };
                let task = queue::InstallTask {
                    name: request.resource_name.clone(),
                    description: Some("来自测试插件".to_string()),
                    icon: Some("https://example.com/i.png".to_string()),
                    device_addr: request.device_addr.clone(),
                    res_type: queue::ResourceType::Watchface,
                    source,
                    watchface_id: Some("12345".to_string()),
                };
                match queue::enqueue_install(task).await {
                    Ok(id) => match queue::cancel_task(id.clone()).await {
                        Ok(()) => InstallOutcome::Redeem(format!("task={id}")),
                        Err(e) => InstallOutcome::Failed(format!("cancel: {e}")),
                    },
                    Err(e) => InstallOutcome::Failed(format!("enqueue: {e}")),
                }
            }
            "enqueue-err" => {
                let task = queue::InstallTask {
                    name: "x".to_string(),
                    description: None,
                    icon: None,
                    device_addr: "ERR".to_string(),
                    res_type: queue::ResourceType::Quickapp,
                    source: queue::InstallSource::Bytes(vec![]),
                    watchface_id: None,
                };
                match queue::enqueue_install(task).await {
                    Ok(id) => InstallOutcome::Redeem(id),
                    Err(e) => InstallOutcome::Failed(e),
                }
            }
            "mass-all" => {
                let kinds = [
                    mass::DataType::Watchface,
                    mass::DataType::Quickapp,
                    mass::DataType::Firmware,
                    mass::DataType::NotificationIcon,
                ];
                for kind in kinds {
                    let options = mass::SendOptions {
                        watchface_id: Some("wf1".to_string()),
                        package_name: Some("com.example.app".to_string()),
                        progress_event: Some("my.progress".to_string()),
                    };
                    if let Err(e) = mass::send(request.device_addr.clone(), kind, vec![9, 8, 7], options).await {
                        return InstallOutcome::Failed(format!("mass: {e}"));
                    }
                }
                InstallOutcome::Queued
            }
            "mass-err" => {
                let options = mass::SendOptions { watchface_id: None, package_name: None, progress_event: None };
                match mass::send("ERR".to_string(), mass::DataType::Quickapp, vec![], options).await {
                    Ok(()) => InstallOutcome::Queued,
                    Err(e) => InstallOutcome::Failed(e),
                }
            }
            "slow" => {
                let options = mass::SendOptions { watchface_id: None, package_name: None, progress_event: None };
                match mass::send("SLOW".to_string(), mass::DataType::Watchface, vec![1], options).await {
                    Ok(()) => InstallOutcome::Queued,
                    Err(e) => InstallOutcome::Failed(e),
                }
            }
            _ => InstallOutcome::Failed("unknown data".to_string()),
        }
    }
}
