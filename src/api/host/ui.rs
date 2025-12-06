use std::collections::HashMap;

use rand::Rng;
use rand::distr::Alphanumeric;
use serde::Serialize;
use tauri::Emitter;
use wasmtime::component::Resource;

use crate::bindings::astrobox::psys_host;

use super::PluginCtx;

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

impl Into<Event> for psys_host::ui::Event {
    fn into(self) -> Event {
        match self {
            psys_host::ui::Event::Click => Event::CLICK,
            psys_host::ui::Event::Hover => Event::HOVER,
            psys_host::ui::Event::Change => Event::CHANGE,
            psys_host::ui::Event::PointerDown => Event::POINTERDOWN,
            psys_host::ui::Event::PointerUp => Event::POINTERUP,
            psys_host::ui::Event::PointerMove => Event::POINTERMOVE,
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
impl Into<ElementType> for psys_host::ui::ElementType {
    fn into(self) -> ElementType {
        match self {
            psys_host::ui::ElementType::Button => ElementType::BUTTON,
            psys_host::ui::ElementType::Image => ElementType::IMAGE,
            psys_host::ui::ElementType::Video => ElementType::VIDEO,
            psys_host::ui::ElementType::Audio => ElementType::AUDIO,
            psys_host::ui::ElementType::Svg => ElementType::SVG,
            psys_host::ui::ElementType::Div => ElementType::DIV,
            psys_host::ui::ElementType::Span => ElementType::SPAN,
            psys_host::ui::ElementType::P => ElementType::P,
        }
    }
}

impl Element {
    fn new(type_: ElementType, content: Option<String>) -> Self {
        Self {
            id: rand::rng()
                .sample_iter(Alphanumeric)
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

impl psys_host::ui::Host for PluginCtx {
    fn render(&mut self, element: Resource<Element>) -> wasmtime::Result<()> {
        let el = self.table.delete(element)?;
        let json = serde_json::to_string(&el);

        let _ = self.app_handle.emit(
            "plugin-ui-render",
            serde_json::json!({
                "name": self.plugin_name(),
                "ui": json.unwrap()
            }),
        );

        Ok(())
    }
}
impl psys_host::ui::HostElement for PluginCtx {
    fn new(
        &mut self,
        element_type: psys_host::ui::ElementType,
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
        event: psys_host::ui::Event,
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
