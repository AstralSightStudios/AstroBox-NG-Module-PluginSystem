use std::collections::HashMap;

use rand::Rng;
use rand::distr::Alphanumeric;
use serde::Serialize;
use tauri::Emitter;
use wasmtime::component::Resource;

use crate::bindings::astrobox::psys_host;

use crate::api::host::PluginCtx;

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
    #[serde(rename = "KEY-DOWN")]
    KEYDOWN,
    #[serde(rename = "KEY-UP")]
    KEYUP,
    #[serde(rename = "LONG-PRESS")]
    LONGPRESS,
}

impl Into<Event> for psys_host::ui_v3::Event {
    fn into(self) -> Event {
        match self {
            psys_host::ui_v3::Event::Click => Event::CLICK,
            psys_host::ui_v3::Event::Hover => Event::HOVER,
            psys_host::ui_v3::Event::Change => Event::CHANGE,
            psys_host::ui_v3::Event::Input => Event::INPUT,
            psys_host::ui_v3::Event::Focus => Event::FOCUS,
            psys_host::ui_v3::Event::Blur => Event::BLUR,
            psys_host::ui_v3::Event::MouseEnter => Event::MOUSEENTER,
            psys_host::ui_v3::Event::MouseLeave => Event::MOUSELEAVE,
            psys_host::ui_v3::Event::PointerDown => Event::POINTERDOWN,
            psys_host::ui_v3::Event::PointerUp => Event::POINTERUP,
            psys_host::ui_v3::Event::PointerMove => Event::POINTERMOVE,
            psys_host::ui_v3::Event::KeyDown => Event::KEYDOWN,
            psys_host::ui_v3::Event::KeyUp => Event::KEYUP,
            psys_host::ui_v3::Event::LongPress => Event::LONGPRESS,
        }
    }
}
#[derive(Clone, Serialize)]
enum ElementType {
    BUTTON,
    INPUT,
    TEXTAREA,
    SWITCH,
    SLIDER,
    PROGRESS,
    SELECT,
    OPTION,
    IMAGE,
    VIDEO,
    AUDIO,
    SVG,
    DIV,
    SPAN,
    P,
    GRID,
    #[serde(rename = "SCROLL_AREA")]
    SCROLLAREA,
    LIST,
    #[serde(rename = "LIST_ITEM")]
    LISTITEM,
    CODE,
    CARD,
    #[serde(rename = "TABS_ROOT")]
    TABSROOT,
    #[serde(rename = "TABS_LIST")]
    TABSLIST,
    #[serde(rename = "TABS_TRIGGER")]
    TABSTRIGGER,
    #[serde(rename = "TABS_CONTENT")]
    TABSCONTENT,
    ICON,
    DIVIDER,
    #[serde(rename = "CONTEXT_MENU_ROOT")]
    CONTEXTMENUROOT,
    #[serde(rename = "CONTEXT_MENU_TRIGGER")]
    CONTEXTMENUTRIGGER,
    #[serde(rename = "CONTEXT_MENU_CONTENT")]
    CONTEXTMENUCONTENT,
    #[serde(rename = "CONTEXT_MENU_ITEM")]
    CONTEXTMENUITEM,
    #[serde(rename = "CONTEXT_MENU_SEPARATOR")]
    CONTEXTMENUSEPARATOR,
    #[serde(rename = "DIALOG_ROOT")]
    DIALOGROOT,
    #[serde(rename = "DIALOG_TRIGGER")]
    DIALOGTRIGGER,
    #[serde(rename = "DIALOG_CONTENT")]
    DIALOGCONTENT,
    #[serde(rename = "DIALOG_TITLE")]
    DIALOGTITLE,
    #[serde(rename = "DIALOG_DESCRIPTION")]
    DIALOGDESCRIPTION,
    #[serde(rename = "DIALOG_CLOSE")]
    DIALOGCLOSE,
    #[serde(rename = "DROPDOWN_MENU_ROOT")]
    DROPDOWNMENUROOT,
    #[serde(rename = "DROPDOWN_MENU_TRIGGER")]
    DROPDOWNMENUTRIGGER,
    #[serde(rename = "DROPDOWN_MENU_CONTENT")]
    DROPDOWNMENUCONTENT,
    #[serde(rename = "DROPDOWN_MENU_ITEM")]
    DROPDOWNMENUITEM,
    #[serde(rename = "DROPDOWN_MENU_SEPARATOR")]
    DROPDOWNMENUSEPARATOR,
    TOOLTIP,
    CHECKBOX,
    SEPARATOR,
    BADGE,
    #[serde(rename = "ALERT_DIALOG_ROOT")]
    ALERTDIALOGROOT,
    #[serde(rename = "ALERT_DIALOG_TRIGGER")]
    ALERTDIALOGTRIGGER,
    #[serde(rename = "ALERT_DIALOG_CONTENT")]
    ALERTDIALOGCONTENT,
    #[serde(rename = "ALERT_DIALOG_TITLE")]
    ALERTDIALOGTITLE,
    #[serde(rename = "ALERT_DIALOG_DESCRIPTION")]
    ALERTDIALOGDESCRIPTION,
    #[serde(rename = "ALERT_DIALOG_ACTION")]
    ALERTDIALOGACTION,
    #[serde(rename = "ALERT_DIALOG_CANCEL")]
    ALERTDIALOGCANCEL,
}
impl Into<ElementType> for psys_host::ui_v3::ElementType {
    fn into(self) -> ElementType {
        match self {
            psys_host::ui_v3::ElementType::Button => ElementType::BUTTON,
            psys_host::ui_v3::ElementType::Input => ElementType::INPUT,
            psys_host::ui_v3::ElementType::Textarea => ElementType::TEXTAREA,
            psys_host::ui_v3::ElementType::Switch => ElementType::SWITCH,
            psys_host::ui_v3::ElementType::Slider => ElementType::SLIDER,
            psys_host::ui_v3::ElementType::Progress => ElementType::PROGRESS,
            psys_host::ui_v3::ElementType::Select => ElementType::SELECT,
            psys_host::ui_v3::ElementType::Option => ElementType::OPTION,
            psys_host::ui_v3::ElementType::Image => ElementType::IMAGE,
            psys_host::ui_v3::ElementType::Video => ElementType::VIDEO,
            psys_host::ui_v3::ElementType::Audio => ElementType::AUDIO,
            psys_host::ui_v3::ElementType::Svg => ElementType::SVG,
            psys_host::ui_v3::ElementType::Div => ElementType::DIV,
            psys_host::ui_v3::ElementType::Span => ElementType::SPAN,
            psys_host::ui_v3::ElementType::P => ElementType::P,
            psys_host::ui_v3::ElementType::Grid => ElementType::GRID,
            psys_host::ui_v3::ElementType::ScrollArea => ElementType::SCROLLAREA,
            psys_host::ui_v3::ElementType::List => ElementType::LIST,
            psys_host::ui_v3::ElementType::ListItem => ElementType::LISTITEM,
            psys_host::ui_v3::ElementType::Code => ElementType::CODE,
            psys_host::ui_v3::ElementType::Card => ElementType::CARD,
            psys_host::ui_v3::ElementType::TabsRoot => ElementType::TABSROOT,
            psys_host::ui_v3::ElementType::TabsList => ElementType::TABSLIST,
            psys_host::ui_v3::ElementType::TabsTrigger => ElementType::TABSTRIGGER,
            psys_host::ui_v3::ElementType::TabsContent => ElementType::TABSCONTENT,
            psys_host::ui_v3::ElementType::Icon => ElementType::ICON,
            psys_host::ui_v3::ElementType::Divider => ElementType::DIVIDER,
            psys_host::ui_v3::ElementType::ContextMenuRoot => ElementType::CONTEXTMENUROOT,
            psys_host::ui_v3::ElementType::ContextMenuTrigger => ElementType::CONTEXTMENUTRIGGER,
            psys_host::ui_v3::ElementType::ContextMenuContent => ElementType::CONTEXTMENUCONTENT,
            psys_host::ui_v3::ElementType::ContextMenuItem => ElementType::CONTEXTMENUITEM,
            psys_host::ui_v3::ElementType::ContextMenuSeparator => ElementType::CONTEXTMENUSEPARATOR,
            psys_host::ui_v3::ElementType::DialogRoot => ElementType::DIALOGROOT,
            psys_host::ui_v3::ElementType::DialogTrigger => ElementType::DIALOGTRIGGER,
            psys_host::ui_v3::ElementType::DialogContent => ElementType::DIALOGCONTENT,
            psys_host::ui_v3::ElementType::DialogTitle => ElementType::DIALOGTITLE,
            psys_host::ui_v3::ElementType::DialogDescription => ElementType::DIALOGDESCRIPTION,
            psys_host::ui_v3::ElementType::DialogClose => ElementType::DIALOGCLOSE,
            psys_host::ui_v3::ElementType::DropdownMenuRoot => ElementType::DROPDOWNMENUROOT,
            psys_host::ui_v3::ElementType::DropdownMenuTrigger => ElementType::DROPDOWNMENUTRIGGER,
            psys_host::ui_v3::ElementType::DropdownMenuContent => ElementType::DROPDOWNMENUCONTENT,
            psys_host::ui_v3::ElementType::DropdownMenuItem => ElementType::DROPDOWNMENUITEM,
            psys_host::ui_v3::ElementType::DropdownMenuSeparator => ElementType::DROPDOWNMENUSEPARATOR,
            psys_host::ui_v3::ElementType::Tooltip => ElementType::TOOLTIP,
            psys_host::ui_v3::ElementType::Checkbox => ElementType::CHECKBOX,
            psys_host::ui_v3::ElementType::Separator => ElementType::SEPARATOR,
            psys_host::ui_v3::ElementType::Badge => ElementType::BADGE,
            psys_host::ui_v3::ElementType::AlertDialogRoot => ElementType::ALERTDIALOGROOT,
            psys_host::ui_v3::ElementType::AlertDialogTrigger => ElementType::ALERTDIALOGTRIGGER,
            psys_host::ui_v3::ElementType::AlertDialogContent => ElementType::ALERTDIALOGCONTENT,
            psys_host::ui_v3::ElementType::AlertDialogTitle => ElementType::ALERTDIALOGTITLE,
            psys_host::ui_v3::ElementType::AlertDialogDescription => ElementType::ALERTDIALOGDESCRIPTION,
            psys_host::ui_v3::ElementType::AlertDialogAction => ElementType::ALERTDIALOGACTION,
            psys_host::ui_v3::ElementType::AlertDialogCancel => ElementType::ALERTDIALOGCANCEL,
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

impl psys_host::ui_v3::Host for PluginCtx {
    fn render(&mut self, id: String, element: Resource<Element>) -> wasmtime::Result<()> {
        let el = take_or_clone_element(self, element)?;
        let json = match serde_json::to_string(&el) {
            Ok(value) => value,
            Err(err) => {
                log::error!(
                    "[pluginsystem] failed to serialize plugin ui render payload for {}: {err}",
                    self.plugin_name()
                );
                return Ok(());
            }
        };

        let _ = self.app_handle.emit(
            "plugin-ui-render",
            serde_json::json!({
                "name": self.plugin_name(),
                "id": id,
                "ui": json
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
impl psys_host::ui_v3::HostElement for PluginCtx {
    fn new(
        &mut self,
        element_type: psys_host::ui_v3::ElementType,
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
        direction: psys_host::ui_v3::FlexDirection,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let value = match direction {
            psys_host::ui_v3::FlexDirection::Row => "row",
            psys_host::ui_v3::FlexDirection::Column => "column",
            psys_host::ui_v3::FlexDirection::RowReverse => "row-reverse",
            psys_host::ui_v3::FlexDirection::ColumnReverse => "column-reverse",
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
        event: psys_host::ui_v3::Event,
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

    fn grid_template_columns(
        &mut self,
        self_: Resource<Element>,
        columns: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("grid-template-columns", columns);
        return_owned_element(self, self_)
    }

    fn gap(
        &mut self,
        self_: Resource<Element>,
        spacing: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("gap", format!("{}px", spacing));
        return_owned_element(self, self_)
    }

    fn max_width(&mut self, self_: Resource<Element>, width: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("max-width", format!("{}px", width));
        return_owned_element(self, self_)
    }

    fn max_height(&mut self, self_: Resource<Element>, height: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("max-height", format!("{}px", height));
        return_owned_element(self, self_)
    }

    fn min_width(&mut self, self_: Resource<Element>, width: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("min-width", format!("{}px", width));
        return_owned_element(self, self_)
    }

    fn min_height(&mut self, self_: Resource<Element>, height: u32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("min-height", format!("{}px", height));
        return_owned_element(self, self_)
    }

    fn flex_grow(&mut self, self_: Resource<Element>, value: f32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("flex-grow", value.to_string());
        return_owned_element(self, self_)
    }

    fn flex_shrink(&mut self, self_: Resource<Element>, value: f32) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("flex-shrink", value.to_string());
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

    fn transform(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("transform", value);
        return_owned_element(self, self_)
    }

    fn transform_origin(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("transform-origin", value);
        return_owned_element(self, self_)
    }

    fn animation(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation", value);
        return_owned_element(self, self_)
    }

    fn animation_name(
        &mut self,
        self_: Resource<Element>,
        name: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-name", name);
        return_owned_element(self, self_)
    }

    fn animation_duration_ms(
        &mut self,
        self_: Resource<Element>,
        ms: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-duration", format!("{}ms", ms));
        return_owned_element(self, self_)
    }

    fn animation_delay_ms(
        &mut self,
        self_: Resource<Element>,
        ms: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-delay", format!("{}ms", ms));
        return_owned_element(self, self_)
    }

    fn animation_easing(
        &mut self,
        self_: Resource<Element>,
        easing: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-timing-function", easing);
        return_owned_element(self, self_)
    }

    fn animation_iteration_count(
        &mut self,
        self_: Resource<Element>,
        count: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-iteration-count", count);
        return_owned_element(self, self_)
    }

    fn animation_direction(
        &mut self,
        self_: Resource<Element>,
        direction: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-direction", direction);
        return_owned_element(self, self_)
    }

    fn animation_fill_mode(
        &mut self,
        self_: Resource<Element>,
        fill_mode: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-fill-mode", fill_mode);
        return_owned_element(self, self_)
    }

    fn animation_play_state(
        &mut self,
        self_: Resource<Element>,
        play_state: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-play-state", play_state);
        return_owned_element(self, self_)
    }

    fn animation_preset(
        &mut self,
        self_: Resource<Element>,
        name: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("animation-preset", name);
        return_owned_element(self, self_)
    }

    fn will_change(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("will-change", value);
        return_owned_element(self, self_)
    }

    fn filter(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("filter", value);
        return_owned_element(self, self_)
    }

    fn backdrop_filter(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("backdrop-filter", value);
        return_owned_element(self, self_)
    }

    fn perspective(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("perspective", value);
        return_owned_element(self, self_)
    }

    fn backface_visibility(
        &mut self,
        self_: Resource<Element>,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        let _ = el.styles.insert("backface-visibility", value);
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
