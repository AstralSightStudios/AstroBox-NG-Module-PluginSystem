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
    without_default_styles: bool,
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
    INPUT,
    FOCUS,
    BLUR,
    MOUSEENTER,
    MOUSELEAVE,
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
            psys_host::ui::Event::Input => Event::INPUT,
            psys_host::ui::Event::Focus => Event::FOCUS,
            psys_host::ui::Event::Blur => Event::BLUR,
            psys_host::ui::Event::MouseEnter => Event::MOUSEENTER,
            psys_host::ui::Event::MouseLeave => Event::MOUSELEAVE,
            psys_host::ui::Event::PointerDown => Event::POINTERDOWN,
            psys_host::ui::Event::PointerUp => Event::POINTERUP,
            psys_host::ui::Event::PointerMove => Event::POINTERMOVE,
        }
    }
}
#[derive(Clone, Serialize)]
enum ElementType {
    BUTTON,
    INPUT,
    SELECT,
    OPTION,
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
            psys_host::ui::ElementType::Input => ElementType::INPUT,
            psys_host::ui::ElementType::Select => ElementType::SELECT,
            psys_host::ui::ElementType::Option => ElementType::OPTION,
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
            without_default_styles: false,
            children: None,
            event_listeners: Vec::new(),
        }
    }
}

fn take_or_clone_element(
    ctx: &mut PluginCtx,
    element: Resource<Element>,
) -> wasmtime::Result<Element> {
    if element.owned() {
        ctx.table.delete(element).map_err(Into::into)
    } else {
        let el = ctx.table.get(&element)?;
        Ok(el.clone())
    }
}

fn return_owned_element(
    ctx: &mut PluginCtx,
    element: Resource<Element>,
) -> wasmtime::Result<Resource<Element>> {
    if element.owned() {
        Ok(element)
    } else {
        let el = ctx.table.get(&element)?;
        ctx.table.push(el.clone()).map_err(Into::into)
    }
}

impl psys_host::ui::Host for PluginCtx {
    fn render(&mut self, id: String, element: Resource<Element>) -> wasmtime::Result<()> {
        let el = take_or_clone_element(self, element)?;
        let json = serde_json::to_string(&el);

        let _ = self.app_handle.emit(
            "plugin-ui-render",
            serde_json::json!({
                "name": self.plugin_name(),
                "id": id,
                "ui": json.unwrap()
            }),
        );

        Ok(())
    }

    fn render_to_text_card(&mut self, id: String, text: String) -> wasmtime::Result<()> {
        let _ = self.app_handle.emit(
            "plugin-ui-render-to-text-card",
            serde_json::json!({
                "name": self.plugin_name(),
                "id": id,
                "text": text
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
        return_owned_element(self, self_)
    }

    fn flex(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("display", "flex".to_string());
        return_owned_element(self, self_)
    }

    fn flex_direction(
        &mut self,
        self_: Resource<Element>,
        direction: psys_host::ui::FlexDirection,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let value = match direction {
            psys_host::ui::FlexDirection::Row => "row",
            psys_host::ui::FlexDirection::Column => "column",
            psys_host::ui::FlexDirection::RowReverse => "row-reverse",
            psys_host::ui::FlexDirection::ColumnReverse => "column-reverse",
        };
        let _ = el.styles.insert("flex-direction", value.to_string());
        return_owned_element(self, self_)
    }

    fn margin(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin", format!("{}px", margin));
        return_owned_element(self, self_)
    }

    fn margin_top(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-top", format!("{}px", margin));
        return_owned_element(self, self_)
    }

    fn margin_bottom(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-bottom", format!("{}px", margin));
        return_owned_element(self, self_)
    }

    fn margin_left(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-left", format!("{}px", margin));
        return_owned_element(self, self_)
    }

    fn margin_right(
        &mut self,
        self_: Resource<Element>,
        margin: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("margin-right", format!("{}px", margin));
        return_owned_element(self, self_)
    }

    fn padding(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding", format!("{}px", padding));
        return_owned_element(self, self_)
    }

    fn padding_top(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-top", format!("{}px", padding));
        return_owned_element(self, self_)
    }

    fn padding_bottom(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-bottom", format!("{}px", padding));
        return_owned_element(self, self_)
    }

    fn padding_left(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-left", format!("{}px", padding));
        return_owned_element(self, self_)
    }

    fn padding_right(
        &mut self,
        self_: Resource<Element>,
        padding: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("padding-right", format!("{}px", padding));
        return_owned_element(self, self_)
    }

    fn align_center(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "center".to_string());
        return_owned_element(self, self_)
    }

    fn align_end(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "flex-end".to_string());
        return_owned_element(self, self_)
    }

    fn align_start(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("align-items", "flex-start".to_string());
        return_owned_element(self, self_)
    }

    fn justify_center(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("justify-content", "center".to_string());
        return_owned_element(self, self_)
    }

    fn justify_start(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el
            .styles
            .insert("justify-content", "flex-start".to_string());
        return_owned_element(self, self_)
    }

    fn justify_end(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("justify-content", "flex-end".to_string());
        return_owned_element(self, self_)
    }

    fn bg(
        &mut self,
        self_: Resource<Element>,
        color: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("background", color);
        return_owned_element(self, self_)
    }

    fn text_color(
        &mut self,
        self_: Resource<Element>,
        color: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("color", color);
        return_owned_element(self, self_)
    }

    fn size(&mut self, self_: Resource<Element>, size: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("font-size", format!("{}px", size));
        return_owned_element(self, self_)
    }

    fn width(
        &mut self,
        self_: Resource<Element>,
        width: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.width = Some(width);
        let _ = el.styles.insert("width", format!("{}px", width));
        return_owned_element(self, self_)
    }

    fn width_full(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.width = None;
        let _ = el.styles.insert("width", "100%".to_string());
        return_owned_element(self, self_)
    }

    fn width_half(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.width = None;
        let _ = el.styles.insert("width", "50%".to_string());
        return_owned_element(self, self_)
    }

    fn height(
        &mut self,
        self_: Resource<Element>,
        height: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.height = Some(height);
        let _ = el.styles.insert("height", format!("{}px", height));
        return_owned_element(self, self_)
    }

    fn height_full(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.height = None;
        let _ = el.styles.insert("height", "100%".to_string());
        return_owned_element(self, self_)
    }

    fn height_half(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.height = None;
        let _ = el.styles.insert("height", "50%".to_string());
        return_owned_element(self, self_)
    }

    fn radius(
        &mut self,
        self_: Resource<Element>,
        radius: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("border-radius", format!("{}px", radius));
        return_owned_element(self, self_)
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
        return_owned_element(self, self_)
    }

    fn child(
        &mut self,
        self_: Resource<Element>,
        child: Resource<Element>,
    ) -> wasmtime::Result<Resource<Element>> {
        let child_el = take_or_clone_element(self, child)?;
        let el = self.table.get_mut(&self_)?;
        if let Some(children) = &mut el.children {
            children.push(child_el);
        } else {
            el.children = Some(vec![child_el]);
        }
        return_owned_element(self, self_)
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
        return_owned_element(self, self_)
    }

    fn drop(&mut self, rep: Resource<Element>) -> wasmtime::Result<()> {
        if rep.owned() {
            let el = self.table.delete(rep)?;
            Ok(drop(el))
        } else {
            Ok(())
        }
    }

    fn relative(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("position", "relative".to_string());
        return_owned_element(self, self_)
    }

    fn absolute(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("position", "absolute".to_string());
        return_owned_element(self, self_)
    }

    fn top(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("top", format!("{}px", position));
        return_owned_element(self, self_)
    }

    fn bottom(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("bottom", format!("{}px", position));
        return_owned_element(self, self_)
    }

    fn left(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("left", format!("{}px", position));
        return_owned_element(self, self_)
    }

    fn right(
        &mut self,
        self_: Resource<Element>,
        position: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("right", format!("{}px", position));
        return_owned_element(self, self_)
    }

    fn opacity(
        &mut self,
        self_: Resource<Element>,
        opacity: f32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("opacity", format!("{}", opacity));
        return_owned_element(self, self_)
    }

    fn transition(
        &mut self,
        self_: Resource<Element>,
        transition: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("transition", transition);
        return_owned_element(self, self_)
    }

    fn without_default_styles(
        &mut self,
        self_: Resource<Element>,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.without_default_styles = true;
        return_owned_element(self, self_)
    }

    fn z_index(&mut self, self_: Resource<Element>, z: i32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("z-index", z.to_string());
        return_owned_element(self, self_)
    }

    fn disabled(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("disabled", "true".to_string());
        return_owned_element(self, self_)
    }
}
