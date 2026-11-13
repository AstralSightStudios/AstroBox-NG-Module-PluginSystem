//! 资源「下载/安装」按钮被插件接管时，宿主与插件之间传递的请求与结果。
//!
//! 前端（Tauri 命令）用 JSON 收发，插件侧是 WIT 的 `astrobox:psys-plugin-v4/resource-install`，
//! 两头的类型在这里各转一次。资源作者写在 manifest 里的 `data` 是不可信输入，
//! 宿主只保证它是合法 JSON 且不超过 [`MAX_DATA_BYTES`]，内容一概不解读。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 给插件处理一次点击的时限：用户在界面上等着，但插件可能要弹窗、联网、入队。
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(180);

/// 作者声明的 `data` 序列化后的上限。
pub const MAX_DATA_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInstallRequest {
    pub request_id: String,
    pub provider: String,
    pub resource_id: String,
    pub resource_name: String,
    pub resource_type: String,
    pub download_key: String,
    pub version: String,
    #[serde(default)]
    pub version_code: Option<u32>,
    pub device_addr: String,
    #[serde(default)]
    pub is_update: bool,
    /// `ext.pluginInstall.data` 的 JSON 文本；没写就是 `null`。
    #[serde(default = "null_json")]
    pub data: String,
}

fn null_json() -> String {
    "null".to_string()
}

/// 插件没法接这次点击的原因（宿主判断，插件不参与）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// 没装这个插件。
    Missing,
    /// 装了但被用户停用。
    Disabled,
    /// 装了但还没跑起来（加载中或加载失败）。
    NotLoaded,
    /// 插件不是 Level 4，或没实现 `resource` 导出。
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResourceInstallOutcome {
    Queued,
    Redeem { message: String },
    Pass,
    Failed { message: String },
    /// 由宿主给出，不会来自插件。
    Unavailable { reason: UnavailableReason },
}

impl ResourceInstallRequest {
    /// 入口检查：必填项非空、`data` 是合法 JSON 且不过大。
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("requestId", &self.request_id),
            ("provider", &self.provider),
            ("resourceId", &self.resource_id),
            ("deviceAddr", &self.device_addr),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} is empty"));
            }
        }
        if self.data.len() > MAX_DATA_BYTES {
            return Err("ext.pluginInstall.data is too large".to_string());
        }
        if serde_json::from_str::<serde_json::Value>(&self.data).is_err() {
            return Err("ext.pluginInstall.data is not valid JSON".to_string());
        }
        Ok(())
    }
}

impl From<crate::v4::bindings_resource_install::exports::astrobox::psys_plugin_v4::resource_install::InstallOutcome>
    for ResourceInstallOutcome
{
    fn from(
        outcome: crate::v4::bindings_resource_install::exports::astrobox::psys_plugin_v4::resource_install::InstallOutcome,
    ) -> Self {
        use crate::v4::bindings_resource_install::exports::astrobox::psys_plugin_v4::resource_install::InstallOutcome as Wit;
        match outcome {
            Wit::Queued => Self::Queued,
            Wit::Redeem(message) => Self::Redeem { message },
            Wit::Pass => Self::Pass,
            Wit::Failed(message) => Self::Failed { message },
        }
    }
}

impl From<ResourceInstallRequest>
    for crate::v4::bindings_resource_install::exports::astrobox::psys_plugin_v4::resource_install::InstallRequest
{
    fn from(request: ResourceInstallRequest) -> Self {
        Self {
            request_id: request.request_id,
            provider: request.provider,
            resource_id: request.resource_id,
            resource_name: request.resource_name,
            resource_type: request.resource_type,
            download_key: request.download_key,
            version: request.version,
            version_code: request.version_code,
            device_addr: request.device_addr,
            is_update: request.is_update,
            data: request.data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ResourceInstallRequest {
        ResourceInstallRequest {
            request_id: "r1".into(),
            provider: "OfficialV2".into(),
            resource_id: "res".into(),
            resource_name: "Face".into(),
            resource_type: "watchface".into(),
            download_key: "p67".into(),
            version: "1.0.0".into(),
            version_code: Some(3),
            device_addr: "AA:BB".into(),
            is_update: false,
            data: "{\"sku\":\"x\"}".into(),
        }
    }

    #[test]
    fn a_complete_request_is_accepted() {
        assert!(request().validate().is_ok());
        let mut no_data = request();
        no_data.data = "null".into();
        assert!(no_data.validate().is_ok());
    }

    #[test]
    fn required_fields_and_data_are_checked() {
        for mutate in [
            (|r: &mut ResourceInstallRequest| r.request_id = " ".into()) as fn(&mut ResourceInstallRequest),
            |r| r.provider = "".into(),
            |r| r.resource_id = "".into(),
            |r| r.device_addr = "  ".into(),
            |r| r.data = "{not json".into(),
            |r| r.data = format!("\"{}\"", "a".repeat(MAX_DATA_BYTES)),
        ] {
            let mut r = request();
            mutate(&mut r);
            assert!(r.validate().is_err());
        }
    }

    #[test]
    fn request_fields_default_when_missing_from_json() {
        let parsed: ResourceInstallRequest = serde_json::from_str(
            r#"{"requestId":"r","provider":"p","resourceId":"i","resourceName":"n","resourceType":"watchface","downloadKey":"k","version":"1","deviceAddr":"a"}"#,
        )
        .unwrap();
        assert_eq!(parsed.data, "null");
        assert!(!parsed.is_update);
        assert_eq!(parsed.version_code, None);
    }

    #[test]
    fn outcomes_serialise_with_a_kind_tag() {
        assert_eq!(serde_json::to_string(&ResourceInstallOutcome::Queued).unwrap(), r#"{"kind":"queued"}"#);
        assert_eq!(
            serde_json::to_string(&ResourceInstallOutcome::Redeem { message: "go".into() }).unwrap(),
            r#"{"kind":"redeem","message":"go"}"#
        );
        assert_eq!(serde_json::to_string(&ResourceInstallOutcome::Pass).unwrap(), r#"{"kind":"pass"}"#);
        assert_eq!(
            serde_json::to_string(&ResourceInstallOutcome::Failed { message: "no".into() }).unwrap(),
            r#"{"kind":"failed","message":"no"}"#
        );
        assert_eq!(
            serde_json::to_string(&ResourceInstallOutcome::Unavailable { reason: UnavailableReason::NotLoaded }).unwrap(),
            r#"{"kind":"unavailable","reason":"not_loaded"}"#
        );
    }
}
