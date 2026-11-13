wit_bindgen::generate!({
    path: "../../wit",
    world: "psys-world-v4",
    generate_all,
});

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
