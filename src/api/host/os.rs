use crate::bindings::astrobox::psys_host;
use chrono::Local;
use wasmtime::component::{Access, FutureReader};

use super::{HostString, PluginCtx, ReadyFuture};

impl psys_host::os::Host for PluginCtx {}

impl psys_host::os::HostWithStore for PluginCtx {
    fn arch<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut store, || std::env::consts::ARCH.to_string())
    }

    fn hostname<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut store, || {
            whoami::hostname().unwrap_or_else(|_| "unknown-host".to_string())
        })
    }

    fn locale<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut store, || {
            sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string())
        })
    }

    fn platform<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut store, || os_info::get().os_type().to_string())
    }

    fn version<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        make_string_future(&mut store, || os_info::get().version().to_string())
    }

    fn astrobox_language<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        let future = FutureReader::new(
            &mut store,
            ReadyFuture::ok(default_astrobox_language().into()),
        );
        future.expect("failed to create host future reader")
    }

    fn appearance<T>(mut store: Access<'_, T, Self>) -> FutureReader<HostString> {
        let future = FutureReader::new(&mut store, ReadyFuture::ok("dark".to_string().into()));
        future.expect("failed to create host future reader")
    }

    fn timezone_offset_minutes<T>(mut store: Access<'_, T, Self>) -> FutureReader<i32> {
        let offset_seconds = Local::now().offset().local_minus_utc();
        let future = FutureReader::new(&mut store, ReadyFuture::ok(offset_seconds / 60));
        future.expect("failed to create host future reader")
    }
}

fn make_string_future<T, F>(
    store: &mut Access<'_, T, PluginCtx>,
    producer: F,
) -> FutureReader<HostString>
where
    F: FnOnce() -> String + Send + 'static,
{
    let future = FutureReader::new(store, ReadyFuture::ok(producer().into()));
    future.expect("failed to create host future reader")
}

fn default_astrobox_language() -> String {
    let locale = sys_locale::get_locale()
        .unwrap_or_default()
        .replace('-', "_");
    match locale.as_str() {
        "zh_CN" | "zh_HK" | "zh_TW" | "en_US" | "ja" | "lzh" | "hi" | "pt_BR" | "ru" | "zh_Cat"
        | "zh_Meme" => locale,
        _ => "en_US".to_string(),
    }
}
