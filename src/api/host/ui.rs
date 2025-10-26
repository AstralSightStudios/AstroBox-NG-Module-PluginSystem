use crate::bindings::astrobox::psys_host;
use anyhow::Error;
use wasmtime::component::{Accessor, FutureReader};

use super::{HostString, PluginCtx};

impl psys_host::ui::Host for PluginCtx {}

impl psys_host::ui::HostWithStore for PluginCtx {
    fn show_dialog<T>(
        accessor: &Accessor<T, Self>,
        dialog_type: psys_host::ui::DialogType,
        style: psys_host::ui::DialogStyle,
        info: psys_host::ui::DialogInfo,
    ) -> impl core::future::Future<Output = FutureReader<psys_host::ui::DialogResult>> + Send {
        let instance = accessor.instance();
        let future = accessor.with(|mut access| {
            FutureReader::new(instance, &mut access, async move {
                let _ = dialog_type;
                let _ = style;
                let _ = info;
                Ok::<psys_host::ui::DialogResult, Error>(psys_host::ui::DialogResult {
                    clicked_btn_id: HostString::default(),
                    input_result: HostString::default(),
                })
            })
        });
        async move { future }
    }
}
