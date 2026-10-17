//! 插件读取当前 AstroBox 账号资料的来源。
//!
//! 账号提供方可能不在本进程的账号注册表里（宿主把账号逻辑放在可热更新的逻辑层时，注册表在逻辑层
//! 实例里），宿主可以在启动时登记一个解析器；没有登记时回退到 `account` crate 的注册表。

use std::sync::{Arc, OnceLock};

use account::models::PublicAccountProfile;
use futures_util::future::BoxFuture;

/// 返回当前 AstroBox 账号的公开资料。错误文案只记日志，不会原样交给插件。
pub type ProfileResolver =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Option<PublicAccountProfile>, String>> + Send + Sync>;

static RESOLVER: OnceLock<ProfileResolver> = OnceLock::new();

/// 账号提供方尚未登记。这条错误原样交给插件，解析器遇到同样情况也应返回它。
pub const NOT_INITIALIZED: &str = "AstroBox account provider is not initialized";

/// 登记解析器（只能登记一次）。
pub fn set_profile_resolver(resolver: ProfileResolver) {
    let _ = RESOLVER.set(resolver);
}

pub(crate) async fn current_profile() -> Result<Option<PublicAccountProfile>, String> {
    if let Some(resolver) = RESOLVER.get() {
        return resolver().await;
    }
    let Some(provider) = account::get_account_provider("astrobox").await else {
        return Err(NOT_INITIALIZED.into());
    };
    provider.public_profile().await.map_err(|err| format!("{err:#}"))
}
