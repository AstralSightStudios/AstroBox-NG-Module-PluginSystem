use std::time::{SystemTime, UNIX_EPOCH};

use corelib::device::notification::{self as core, LiveActivity, NotificationContent};
use pb::xiaomi::protocol::notify_data;
use serde_json::json;
use wasmtime_v4::{self as wasmtime, component::Accessor};

use crate::{
    api::host::permission::check_permission_declared,
    v4::{bindings::astrobox::psys_host_v4::notification, ctx::PluginCtxV4},
};

impl notification::Host for PluginCtxV4 {}

impl notification::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn send(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        message: notification::Message,
    ) -> wasmtime::Result<Result<(), String>> {
        let Some(plugin) = authorize(accessor, &addr).await else {
            return Ok(Err("permission denied".into()));
        };
        let timestamp_ms = match message.timestamp_ms {
            Some(value) => value,
            None => match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(value) => value.as_millis() as u64,
                Err(_) => return Ok(Err("system clock is before Unix epoch".into())),
            },
        };
        let content = NotificationContent {
            id: message.id,
            app_name: message.app_name,
            title: message.title,
            sub_title: message.sub_title,
            text: message.body,
            timestamp_ms,
            live_activity: message.live_activity.map(to_activity),
        };
        Ok(core::send(addr, plugin, content)
            .await
            .map_err(|err| err.to_string()))
    }

    async fn remove(
        accessor: &Accessor<PluginCtxV4, Self>,
        addr: String,
        id: u32,
    ) -> wasmtime::Result<Result<(), String>> {
        let Some(plugin) = authorize(accessor, &addr).await else {
            return Ok(Err("permission denied".into()));
        };
        Ok(core::remove(addr, plugin, id)
            .await
            .map_err(|err| err.to_string()))
    }
}

async fn authorize(accessor: &Accessor<PluginCtxV4, PluginCtxV4>, addr: &str) -> Option<String> {
    let (app, plugin, permissions) = accessor.with(|mut access| {
        let ctx = access.get();
        (
            ctx.app_handle(),
            ctx.plugin_name().to_string(),
            ctx.permissions(),
        )
    });
    check_permission_declared(
        &app,
        &permissions,
        "notification",
        json!({"plugin": plugin, "addr": addr}),
    )
    .await
    .then_some(plugin)
}

fn to_text(value: notification::Text) -> notify_data::Text {
    notify_data::Text {
        chars: value.chars,
        color: value.color,
    }
}

fn to_progress(value: notification::Progress) -> notify_data::Progress {
    notify_data::Progress {
        section_count: value.section_count,
        progress: value.progress,
        color: value.color,
    }
}

fn to_info(value: notification::Info) -> notify_data::Info {
    notify_data::Info {
        title: to_text(value.title),
        sub_title: value.sub_title.map(to_text),
        content: value.content.map(to_text),
        sub_content: value.sub_content.map(to_text),
        special_title: value.special_title.map(to_text),
        special_title_bg: value.special_title_bg,
    }
}

fn to_activity(value: notification::LiveActivity) -> LiveActivity {
    match value {
        notification::LiveActivity::Focus(value) => LiveActivity::Focus(notify_data::Focus {
            style: value.style,
            title: to_text(value.title),
            content: to_text(value.content),
            desc: to_text(value.desc),
            progress: value.progress.map(to_progress),
            updatable: Some(value.updatable),
            sequence: Some(value.sequence),
        }),
        notification::LiveActivity::FocusV2(value) => LiveActivity::FocusV2(notify_data::FocusV2 {
            scene: value.scene,
            ticker: value.ticker,
            basic_info: to_info(value.basic_info),
            hint_info: value.hint_info.map(to_info),
            progress: value.progress.map(to_progress),
            updatable: Some(value.updatable),
            sequence: Some(value.sequence),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_v2_maps_all_optional_fields_without_loss() {
        let text = || notification::Text {
            chars: "活动".into(),
            color: Some(vec![1, 2, 3, 4]),
        };
        let info = || notification::Info {
            title: text(),
            sub_title: Some(text()),
            content: Some(text()),
            sub_content: Some(text()),
            special_title: Some(text()),
            special_title_bg: Some(vec![5, 6]),
        };
        let input = notification::LiveActivity::FocusV2(notification::FocusV2 {
            scene: 12,
            ticker: "进度".into(),
            basic_info: info(),
            hint_info: Some(info()),
            progress: Some(notification::Progress {
                section_count: 10,
                progress: 3,
                color: Some(vec![7, 8]),
            }),
            updatable: true,
            sequence: 9,
        });
        let LiveActivity::FocusV2(value) = to_activity(input) else {
            panic!()
        };
        assert_eq!(value.scene, 12);
        assert_eq!(value.ticker, "进度");
        assert_eq!(value.sequence, Some(9));
        assert_eq!(value.updatable, Some(true));
        assert_eq!(value.basic_info, value.hint_info.unwrap());
        assert_eq!(value.basic_info.title.color, Some(vec![1, 2, 3, 4]));
        assert_eq!(value.basic_info.special_title_bg, Some(vec![5, 6]));
        assert_eq!(value.basic_info.sub_title.as_ref().unwrap().chars, "活动");
        assert_eq!(value.basic_info.content, value.basic_info.sub_content);
        assert_eq!(value.basic_info.content, value.basic_info.special_title);
        let progress = value.progress.unwrap();
        assert_eq!(
            (progress.section_count, progress.progress, progress.color),
            (10, 3, Some(vec![7, 8]))
        );
    }
}
