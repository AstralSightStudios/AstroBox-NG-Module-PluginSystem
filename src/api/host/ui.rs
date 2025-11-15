use std::collections::HashMap;

use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};

use anyhow::Error;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};
use tokio::sync::oneshot;
use wasmtime::component::{Accessor, FutureReader, Resource};

use crate::bindings::astrobox::psys_host;

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
            let app_handle = {
                let ctx = access.get();
                ctx.app_handle()
            };
            FutureReader::new(instance, &mut access, async move {
                match (dialog_type, style) {
                    (
                        psys_host::ui::DialogType::Alert,
                        psys_host::ui::DialogStyle::System,
                    ) => show_system_alert(app_handle, info).await,
                    _ => {
                        log::warn!(
                            "ui::show_dialog receive an unimplemented combination, type={:?} style={:?}, and return the default result",
                            dialog_type,
                            style
                        );
                        Ok(default_dialog_result())
                    }
                }
            })
        });
        async move { future }
    }
}

async fn show_system_alert(
    app_handle: AppHandle,
    info: psys_host::ui::DialogInfo,
) -> Result<psys_host::ui::DialogResult, Error> {
    let title: String = info.title.into();
    let message: String = info.content.into();
    let mut buttons: Vec<ButtonSpec> = info.buttons.into_iter().map(ButtonSpec::from).collect();

    if buttons.is_empty() {
        log::debug!(
            "The system pop-up window does not provide a custom button, so use the default OK"
        );
    }

    buttons.sort_by(|a, b| b.primary.cmp(&a.primary));

    if buttons.len() > 3 {
        log::warn!(
            "The number of system pop-up buttons is {}, which exceeds the native limit and is truncated to the first three buttons",
            buttons.len()
        );
        buttons.truncate(3);
    }

    let (button_config, mapping) = build_button_config(buttons);
    let (tx, rx) = oneshot::channel();

    app_handle
        .dialog()
        .message(message)
        .title(title)
        .buttons(button_config)
        .show_with_result(move |result| {
            let clicked_btn_id = resolve_dialog_result(result, &mapping);
            let dialog_result = psys_host::ui::DialogResult {
                clicked_btn_id,
                input_result: HostString::default(),
            };
            let _ = tx.send(dialog_result);
        });

    match rx.await {
        Ok(result) => Ok(result),
        Err(err) => {
            log::error!("Waiting for system pop-up result failed: {}", err);
            Ok(default_dialog_result())
        }
    }
}

fn build_button_config(buttons: Vec<ButtonSpec>) -> (MessageDialogButtons, Vec<ButtonSpec>) {
    if buttons.is_empty() {
        return (MessageDialogButtons::Ok, Vec::new());
    }

    let config = match buttons.len() {
        1 => MessageDialogButtons::OkCustom(buttons[0].label.clone()),
        2 => {
            MessageDialogButtons::OkCancelCustom(buttons[0].label.clone(), buttons[1].label.clone())
        }
        3 => MessageDialogButtons::YesNoCancelCustom(
            buttons[0].label.clone(),
            buttons[1].label.clone(),
            buttons[2].label.clone(),
        ),
        _ => MessageDialogButtons::Ok,
    };

    (config, buttons)
}

fn resolve_dialog_result(result: MessageDialogResult, buttons: &[ButtonSpec]) -> HostString {
    let clicked = match result {
        MessageDialogResult::Custom(label) => buttons
            .iter()
            .find(|b| b.label == label)
            .map(|b| b.id.clone()),
        MessageDialogResult::Ok | MessageDialogResult::Yes => buttons.first().map(|b| b.id.clone()),
        MessageDialogResult::No => buttons.get(1).map(|b| b.id.clone()),
        MessageDialogResult::Cancel => {
            if buttons.len() >= 3 {
                buttons.get(2).map(|b| b.id.clone())
            } else {
                buttons.last().map(|b| b.id.clone())
            }
        }
    }
    .unwrap_or_default();

    clicked.into()
}

fn default_dialog_result() -> psys_host::ui::DialogResult {
    psys_host::ui::DialogResult {
        clicked_btn_id: HostString::default(),
        input_result: HostString::default(),
    }
}

#[derive(Clone)]
struct ButtonSpec {
    id: String,
    label: String,
    primary: bool,
}

impl From<psys_host::ui::DialogButton> for ButtonSpec {
    fn from(button: psys_host::ui::DialogButton) -> Self {
        Self {
            id: button.id.into(),
            label: button.content.into(),
            primary: button.primary,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Element {
    id: String,
    r#type: ElementType,
    content: Option<String>,
    event_listeners: Vec<EventListener>,
    styles: HashMap<&'static str, String>,
    width: Option<u32>,
    height: Option<u32>,
    children: Option<Vec<Element>>,
}

#[derive(Clone, Serialize)]
struct EventListener {
    id: String,
    event: Event,
}

#[derive(Clone, Serialize)]
enum Event {
    CLICK,
    HOVER,
    CHANGE,
    POINTERDOWN,
    POINTERUP,
    POINTERMOVE,
}

impl Into<Event> for psys_host::ui2::Event {
    fn into(self) -> Event {
        match self {
            psys_host::ui2::Event::Click => Event::CLICK,
            psys_host::ui2::Event::Hover => Event::HOVER,
            psys_host::ui2::Event::Change => Event::CHANGE,
            psys_host::ui2::Event::PointerDown => Event::POINTERDOWN,
            psys_host::ui2::Event::PointerUp => Event::POINTERUP,
            psys_host::ui2::Event::PointerMove => Event::POINTERMOVE,
        }
    }
}
#[derive(Clone, Serialize)]
enum ElementType {
    BUTTON,
    IMAGE,
    VIDEO,
    AUDIO,
    SVG,
    DIV,
    SPAN,
    P,
}
impl Into<ElementType> for psys_host::ui2::ElementType {
    fn into(self) -> ElementType {
        match self {
            psys_host::ui2::ElementType::Button => ElementType::BUTTON,
            psys_host::ui2::ElementType::Image => ElementType::IMAGE,
            psys_host::ui2::ElementType::Video => ElementType::VIDEO,
            psys_host::ui2::ElementType::Audio => ElementType::AUDIO,
            psys_host::ui2::ElementType::Svg => ElementType::SVG,
            psys_host::ui2::ElementType::Div => ElementType::DIV,
            psys_host::ui2::ElementType::Span => ElementType::SPAN,
            psys_host::ui2::ElementType::P => ElementType::P,
        }
    }
}

impl Element {
    fn new(type_: ElementType, content: Option<String>) -> Self {
        Self {
            id: thread_rng()
                .sample_iter(&Alphanumeric)
                .take(16)
                .map(char::from)
                .collect(),
            r#type: type_,
            content,
            styles: HashMap::new(),
            width: None,
            height: None,
            children: None,
            event_listeners: Vec::new(),
        }
    }
}

impl psys_host::ui2::Host for PluginCtx {
    fn render(&mut self, element: Resource<Element>) -> wasmtime::Result<()> {
        let el = self.table.delete(element)?;
        let json = serde_json::to_string(&el);

        Ok(())
    }
}
impl psys_host::ui2::HostElement for PluginCtx {
    fn new(
        &mut self,
        element_type: psys_host::ui2::ElementType,
        content: Option<String>,
    ) -> wasmtime::Result<Resource<Element>> {
        let id = self
            .table
            .push(Element::new(element_type.into(), content))?;
        Ok(id)
    }

    fn content(
        &mut self,
        self_: Resource<Element>,
        content: Option<String>,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.content = content;
        Ok(self_)
    }

    fn flex(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("display", "flex".to_string());
        Ok(self_)
    }

    fn margin(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin", format!("{}px", margin));
        Ok(self_)
    }

    fn margin_top(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-top", format!("{}px", margin));
        Ok(self_)
    }

    fn margin_bottom(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-bottom", format!("{}px", margin));
        Ok(self_)
    }

    fn margin_left(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-left", format!("{}px", margin));
        Ok(self_)
    }

    fn margin_right(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-right", format!("{}px", margin));
        Ok(self_)
    }

    fn padding(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding", format!("{}px", padding));
        Ok(self_)
    }

    fn padding_top(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-top", format!("{}px", padding));
        Ok(self_)
    }

    fn padding_bottom(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-bottom", format!("{}px", padding));
        Ok(self_)
    }

    fn padding_left(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-left", format!("{}px", padding));
        Ok(self_)
    }

    fn padding_right(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-right", format!("{}px", padding));
        Ok(self_)
    }

    fn align_center(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "center".to_string());
        Ok(self_)
    }

    fn align_end(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "flex-end".to_string());
        Ok(self_)
    }

    fn align_start(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "flex-start".to_string());
        Ok(self_)
    }

    fn justify_center(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("justify-content", "center".to_string());
        Ok(self_)
    }

    fn justify_start(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el
            .styles
            .insert("justify-content", "flex-start".to_string());
        Ok(self_)
    }

    fn justify_end(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("justify-content", "flex-end".to_string());
        Ok(self_)
    }

    fn bg(
        &mut self,
        self_: Resource<Element>,
        color: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("background", color);
        Ok(self_)
    }

    fn text_color(
        &mut self,
        self_: Resource<Element>,
        color: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("color", color);
        Ok(self_)
    }

    fn size(&mut self, self_: Resource<Element>, size: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("font-size", format!("{}px", size));
        Ok(self_)
    }

    fn width(
        &mut self,
        self_: Resource<Element>,
        width: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.width = Some(width);
        let _ = el.styles.insert("width", format!("{}px", width));
        Ok(self_)
    }

    fn height(
        &mut self,
        self_: Resource<Element>,
        height: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.height = Some(height);
        let _ = el.styles.insert("height", format!("{}px", height));
        Ok(self_)
    }

    fn radius(
        &mut self,
        self_: Resource<Element>,
        radius: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("border-radius", format!("{}px", radius));
        Ok(self_)
    }

    fn border(
        &mut self,
        self_: Resource<Element>,
        width: u32,
        color: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el
            .styles
            .insert("border", format!("{}px solid {}", width, color));
        Ok(self_)
    }

    fn child(
        &mut self,
        self_: Resource<Element>,
        child: Resource<Element>,
    ) -> wasmtime::Result<Resource<Element>> {
        // Move the child out of the table and append to parent's children
        let child_el = self.table.delete(child)?;
        let el = self.table.get_mut(&self_)?;
        if let Some(children) = &mut el.children {
            children.push(child_el);
        } else {
            el.children = Some(vec![child_el]);
        }
        Ok(self_)
    }

    fn on(
        &mut self,
        self_: Resource<Element>,
        event: psys_host::ui2::Event,
        id: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.event_listeners.push(EventListener {
            id,
            event: event.into(),
        });
        Ok(self_)
    }

    fn drop(&mut self, rep: Resource<Element>) -> wasmtime::Result<()> {
        let el = self.table.delete(rep)?;
        Ok(drop(el))
    }

    fn relative(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("position", "relative".to_string());
        Ok(self_)
    }

    fn absolute(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("position", "absolute".to_string());
        Ok(self_)
    }

    fn top(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("top", format!("{}px", position));
        Ok(self_)
    }

    fn bottom(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("bottom", format!("{}px", position));
        Ok(self_)
    }

    fn left(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("left", format!("{}px", position));
        Ok(self_)
    }

    fn right(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("right", format!("{}px", position));
        Ok(self_)
    }

    fn opacity(
        &mut self,
        self_: Resource<Element>,
        opacity: f32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("opacity", format!("{}", opacity));
        Ok(self_)
    }

    fn transition(
        &mut self,
        self_: Resource<Element>,
        transition: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("transition", transition);
        Ok(self_)
    }

    fn z_index(&mut self, self_: Resource<Element>, z: i32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("z-index", z.to_string());
        Ok(self_)
    }

    fn disabled(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("disabled", "true".to_string());
        Ok(self_)
    }
}
