//! Level 4 的应用内浏览器接口。
//!
//! 真正的平台实现在 `inappbrowser` 插件里（桌面用 Tauri 子 webview 窗口，
//! iOS 用 WKWebView，Android 用 Dialog + WebView），这里只做权限校验与类型转换。

use serde_json::json;
use wasmtime_v4 as wasmtime;
use wasmtime::component::Accessor;

use inappbrowser::{InappbrowserExt, OpenControlledRequest};

use crate::api::host::permission::check_permission_declared;
use crate::v4::bindings::astrobox::psys_host_v4::browser;
use crate::v4::ctx::PluginCtxV4;

/// 打开应用内浏览器所需的权限。
///
/// 这个能力可以看到用户在该页面里输入的一切（包括第三方账号密码），因此**每次
/// open 都要过一次授权**，不像只读接口那样只在首次询问。
const PERMISSION: &str = "browser";

fn to_binding_cookie(cookie: inappbrowser::BrowserCookie) -> browser::Cookie {
    browser::Cookie {
        name: cookie.name,
        value: cookie.value,
        domain: cookie.domain,
        path: cookie.path,
    }
}

impl browser::Host for PluginCtxV4 {}

impl browser::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn open(
        accessor: &Accessor<PluginCtxV4, Self>,
        options: browser::OpenOptions,
    ) -> wasmtime::Result<Result<u32, String>> {
        let (app_handle, plugin_name, permissions) = accessor.with(|mut access| {
            let ctx = access.get();
            (
                ctx.app_handle(),
                ctx.plugin_name().to_string(),
                ctx.permissions(),
            )
        });

        let params = json!({
            "plugin": plugin_name,
            "url": options.url.clone(),
            "interceptPrefixes": options.intercept_prefixes.clone(),
        });
        if !check_permission_declared(&app_handle, permissions.as_ref(), PERMISSION, params).await {
            return Ok(Err("permission denied".to_string()));
        }

        let request = OpenControlledRequest {
            url: options.url,
            title: options.title,
            user_agent: options.user_agent,
            intercept_prefixes: options.intercept_prefixes,
            close_on_intercept: options.close_on_intercept,
            ephemeral: options.ephemeral,
            width: options.width,
            height: options.height,
        };

        Ok(app_handle
            .inappbrowser()
            .open_controlled(request)
            .await
            .map_err(|err| err.to_string()))
    }

    async fn wait_for_intercept(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        timeout_ms: Option<u64>,
    ) -> wasmtime::Result<Result<String, String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .wait_for_intercept(id, timeout_ms)
            .await
            .map_err(|err| err.to_string()))
    }

    async fn navigate(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        url: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .navigate(id, url)
            .await
            .map_err(|err| err.to_string()))
    }

    async fn current_url(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
    ) -> wasmtime::Result<Result<String, String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .current_url(id)
            .map_err(|err| err.to_string()))
    }

    async fn eval(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        script: String,
    ) -> wasmtime::Result<Result<String, String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .eval(id, script)
            .await
            .map_err(|err| err.to_string()))
    }

    async fn get_cookies(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
        url: String,
    ) -> wasmtime::Result<Result<Vec<browser::Cookie>, String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .get_cookies(id, url)
            .await
            .map(|cookies| cookies.into_iter().map(to_binding_cookie).collect())
            .map_err(|err| err.to_string()))
    }

    async fn close(
        accessor: &Accessor<PluginCtxV4, Self>,
        id: u32,
    ) -> wasmtime::Result<Result<(), String>> {
        let app_handle = accessor.with(|mut access| access.get().app_handle());
        Ok(app_handle
            .inappbrowser()
            .close_controlled(id)
            .await
            .map_err(|err| err.to_string()))
    }
}
