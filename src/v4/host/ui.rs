//! Level 4 的 UI 接口实现。
//!
//! 复用 Level 3 的 `Element` 数据结构与前端协议（`plugin-ui-render` 事件），
//! 只是绑定层换成 wasmtime 48。绝大多数构建器方法就是往 styles 里塞一条 CSS，
//! 因此用宏批量生成，避免上百个只差属性名的函数体。

use tauri::Emitter;
use wasmtime_v4 as wasmtime;
use wasmtime::component::{Accessor, Resource};

use crate::api::host::v3::ui::{Element, ElementType, Event};
use crate::v4::bindings::astrobox::psys_host_v4::ui;
use crate::v4::ctx::PluginCtxV4;

impl From<ui::Event> for Event {
    fn from(value: ui::Event) -> Self {
        match value {
            ui::Event::Click => Event::CLICK,
            ui::Event::Hover => Event::HOVER,
            ui::Event::Change => Event::CHANGE,
            ui::Event::Input => Event::INPUT,
            ui::Event::Focus => Event::FOCUS,
            ui::Event::Blur => Event::BLUR,
            ui::Event::MouseEnter => Event::MOUSEENTER,
            ui::Event::MouseLeave => Event::MOUSELEAVE,
            ui::Event::PointerDown => Event::POINTERDOWN,
            ui::Event::PointerUp => Event::POINTERUP,
            ui::Event::PointerMove => Event::POINTERMOVE,
            ui::Event::KeyDown => Event::KEYDOWN,
            ui::Event::KeyUp => Event::KEYUP,
            ui::Event::LongPress => Event::LONGPRESS,
        }
    }
}

impl From<ui::ElementType> for ElementType {
    fn from(value: ui::ElementType) -> Self {
        match value {
            ui::ElementType::Button => ElementType::BUTTON,
            ui::ElementType::Input => ElementType::INPUT,
            ui::ElementType::Textarea => ElementType::TEXTAREA,
            ui::ElementType::Switch => ElementType::SWITCH,
            ui::ElementType::Slider => ElementType::SLIDER,
            ui::ElementType::Progress => ElementType::PROGRESS,
            ui::ElementType::Select => ElementType::SELECT,
            ui::ElementType::Option => ElementType::OPTION,
            ui::ElementType::Image => ElementType::IMAGE,
            ui::ElementType::Video => ElementType::VIDEO,
            ui::ElementType::Audio => ElementType::AUDIO,
            ui::ElementType::Svg => ElementType::SVG,
            ui::ElementType::Div => ElementType::DIV,
            ui::ElementType::Span => ElementType::SPAN,
            ui::ElementType::P => ElementType::P,
            ui::ElementType::Grid => ElementType::GRID,
            ui::ElementType::ScrollArea => ElementType::SCROLLAREA,
            ui::ElementType::Code => ElementType::CODE,
            ui::ElementType::Card => ElementType::CARD,
            ui::ElementType::TabsRoot => ElementType::TABSROOT,
            ui::ElementType::TabsList => ElementType::TABSLIST,
            ui::ElementType::TabsTrigger => ElementType::TABSTRIGGER,
            ui::ElementType::TabsContent => ElementType::TABSCONTENT,
            ui::ElementType::ContextMenuRoot => ElementType::CONTEXTMENUROOT,
            ui::ElementType::ContextMenuTrigger => ElementType::CONTEXTMENUTRIGGER,
            ui::ElementType::ContextMenuContent => ElementType::CONTEXTMENUCONTENT,
            ui::ElementType::ContextMenuItem => ElementType::CONTEXTMENUITEM,
            ui::ElementType::ContextMenuSeparator => ElementType::CONTEXTMENUSEPARATOR,
            ui::ElementType::DialogRoot => ElementType::DIALOGROOT,
            ui::ElementType::DialogTrigger => ElementType::DIALOGTRIGGER,
            ui::ElementType::DialogContent => ElementType::DIALOGCONTENT,
            ui::ElementType::DialogTitle => ElementType::DIALOGTITLE,
            ui::ElementType::DialogDescription => ElementType::DIALOGDESCRIPTION,
            ui::ElementType::DialogClose => ElementType::DIALOGCLOSE,
            ui::ElementType::DropdownMenuRoot => ElementType::DROPDOWNMENUROOT,
            ui::ElementType::DropdownMenuTrigger => ElementType::DROPDOWNMENUTRIGGER,
            ui::ElementType::DropdownMenuContent => ElementType::DROPDOWNMENUCONTENT,
            ui::ElementType::DropdownMenuItem => ElementType::DROPDOWNMENUITEM,
            ui::ElementType::DropdownMenuSeparator => ElementType::DROPDOWNMENUSEPARATOR,
            ui::ElementType::Tooltip => ElementType::TOOLTIP,
            ui::ElementType::Checkbox => ElementType::CHECKBOX,
            ui::ElementType::Separator => ElementType::SEPARATOR,
            ui::ElementType::Badge => ElementType::BADGE,
        }
    }
}

/// 拿到元素的所有权：借用句柄就克隆一份，避免把插件还在用的节点删掉。
fn take_or_clone_element(
    ctx: &mut PluginCtxV4,
    element: Resource<Element>,
) -> wasmtime::Result<Element> {
    if element.owned() {
        ctx.table.delete(element).map_err(Into::into)
    } else {
        let el = ctx.table.get(&element)?;
        Ok(el.clone())
    }
}

/// 构建器方法要返回一个 owned 句柄，借用句柄就复制一份进表。
fn return_owned_element(
    ctx: &mut PluginCtxV4,
    element: Resource<Element>,
) -> wasmtime::Result<Resource<Element>> {
    if element.owned() {
        Ok(element)
    } else {
        let el = ctx.table.get(&element)?;
        ctx.table.push(el.clone()).map_err(Into::into)
    }
}

/// 批量生成「设一条 CSS 然后返回自身」的构建器方法。
macro_rules! style_setters {
    ($($method:ident($($arg:ident : $ty:ty),*) => $css:literal => $value:expr;)*) => {
        $(
            fn $method(
                &mut self,
                self_: Resource<Element>,
                $($arg: $ty),*
            ) -> wasmtime::Result<Resource<Element>> {
                let el = self.table.get_mut(&self_)?;
                el.set_style($css, $value);
                return_owned_element(self, self_)
            }
        )*
    };
}

impl ui::HostElement for PluginCtxV4 {
    fn new(
        &mut self,
        element_type: ui::ElementType,
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
        el.set_content(content);
        return_owned_element(self, self_)
    }

    fn prop(
        &mut self,
        self_: Resource<Element>,
        name: String,
        value: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_prop(name, value);
        return_owned_element(self, self_)
    }

    fn flex_direction(
        &mut self,
        self_: Resource<Element>,
        direction: ui::FlexDirection,
    ) -> wasmtime::Result<Resource<Element>> {
        let value = match direction {
            ui::FlexDirection::Row => "row",
            ui::FlexDirection::Column => "column",
            ui::FlexDirection::RowReverse => "row-reverse",
            ui::FlexDirection::ColumnReverse => "column-reverse",
        };
        let el = self.table.get_mut(&self_)?;
        el.set_style("flex-direction", value.to_string());
        return_owned_element(self, self_)
    }

    // 宽高除了 CSS 还要记像素值，前端按它做布局计算，因此不能走 style_setters。
    fn width(
        &mut self,
        self_: Resource<Element>,
        width: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_width_px(Some(width));
        el.set_style("width", format!("{}px", width));
        return_owned_element(self, self_)
    }

    fn width_full(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_width_px(None);
        el.set_style("width", "100%".to_string());
        return_owned_element(self, self_)
    }

    fn width_half(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_width_px(None);
        el.set_style("width", "50%".to_string());
        return_owned_element(self, self_)
    }

    fn height(
        &mut self,
        self_: Resource<Element>,
        height: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_height_px(Some(height));
        el.set_style("height", format!("{}px", height));
        return_owned_element(self, self_)
    }

    fn height_full(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_height_px(None);
        el.set_style("height", "100%".to_string());
        return_owned_element(self, self_)
    }

    fn height_half(&mut self, self_: Resource<Element>) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_height_px(None);
        el.set_style("height", "50%".to_string());
        return_owned_element(self, self_)
    }

    fn without_default_styles(
        &mut self,
        self_: Resource<Element>,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_without_default_styles();
        return_owned_element(self, self_)
    }

    fn child(
        &mut self,
        self_: Resource<Element>,
        child: Resource<Element>,
    ) -> wasmtime::Result<Resource<Element>> {
        let child_el = take_or_clone_element(self, child)?;
        let el = self.table.get_mut(&self_)?;
        el.push_child(child_el);
        return_owned_element(self, self_)
    }

    fn on(
        &mut self,
        self_: Resource<Element>,
        event: ui::Event,
        id: String,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.push_event_listener(event.into(), id);
        return_owned_element(self, self_)
    }

    fn drop(&mut self, rep: Resource<Element>) -> wasmtime::Result<()> {
        if rep.owned() {
            let _ = self.table.delete(rep)?;
        }
        Ok(())
    }

    style_setters! {
        flex() => "display" => "flex".to_string();

        margin(margin: u32) => "margin" => format!("{}px", margin);
        margin_top(margin: u32) => "margin-top" => format!("{}px", margin);
        margin_bottom(margin: u32) => "margin-bottom" => format!("{}px", margin);
        margin_left(margin: u32) => "margin-left" => format!("{}px", margin);
        margin_right(margin: u32) => "margin-right" => format!("{}px", margin);

        padding(padding: u32) => "padding" => format!("{}px", padding);
        padding_top(padding: u32) => "padding-top" => format!("{}px", padding);
        padding_bottom(padding: u32) => "padding-bottom" => format!("{}px", padding);
        padding_left(padding: u32) => "padding-left" => format!("{}px", padding);
        padding_right(padding: u32) => "padding-right" => format!("{}px", padding);

        align_center() => "align-items" => "center".to_string();
        align_end() => "align-items" => "flex-end".to_string();
        align_start() => "align-items" => "flex-start".to_string();

        justify_center() => "justify-content" => "center".to_string();
        justify_start() => "justify-content" => "flex-start".to_string();
        justify_end() => "justify-content" => "flex-end".to_string();

        bg(color: String) => "background" => color;
        text_color(color: String) => "color" => color;

        size(size: u32) => "font-size" => format!("{}px", size);
        radius(radius: u32) => "border-radius" => format!("{}px", radius);
        border(width: u32, color: String) => "border" => format!("{}px solid {}", width, color);

        relative() => "position" => "relative".to_string();
        absolute() => "position" => "absolute".to_string();

        top(position: u32) => "top" => format!("{}px", position);
        bottom(position: u32) => "bottom" => format!("{}px", position);
        left(position: u32) => "left" => format!("{}px", position);
        right(position: u32) => "right" => format!("{}px", position);

        opacity(opacity: f32) => "opacity" => format!("{}", opacity);
        transition(transition: String) => "transition" => transition;
        transform(value: String) => "transform" => value;
        transform_origin(value: String) => "transform-origin" => value;
        animation(value: String) => "animation" => value;
        animation_name(name: String) => "animation-name" => name;
        animation_duration_ms(ms: u32) => "animation-duration" => format!("{}ms", ms);
        animation_delay_ms(ms: u32) => "animation-delay" => format!("{}ms", ms);
        animation_easing(easing: String) => "animation-timing-function" => easing;
        animation_iteration_count(count: String) => "animation-iteration-count" => count;
        animation_direction(direction: String) => "animation-direction" => direction;
        animation_fill_mode(fill_mode: String) => "animation-fill-mode" => fill_mode;
        animation_play_state(play_state: String) => "animation-play-state" => play_state;
        animation_preset(name: String) => "animation-preset" => name;
        will_change(value: String) => "will-change" => value;
        filter(value: String) => "filter" => value;
        backdrop_filter(value: String) => "backdrop-filter" => value;
        perspective(value: String) => "perspective" => value;
        backface_visibility(value: String) => "backface-visibility" => value;

        autofocus() => "autofocus" => "true".to_string();
        tab_index(index: i32) => "tab-index" => index.to_string();
        z_index(z: i32) => "z-index" => z.to_string();
        disabled() => "disabled" => "true".to_string();

        grid_template_columns(columns: String) => "grid-template-columns" => columns;
        gap(spacing: u32) => "gap" => format!("{}px", spacing);

        max_width(width: u32) => "max-width" => format!("{}px", width);
        max_height(height: u32) => "max-height" => format!("{}px", height);
        min_width(width: u32) => "min-width" => format!("{}px", width);
        min_height(height: u32) => "min-height" => format!("{}px", height);

        scroll_top(position: u32) => "scroll-top" => position.to_string();
        scroll_left(position: u32) => "scroll-left" => position.to_string();
        scroll_behavior(behavior: String) => "scroll-behavior" => behavior;

        flex_grow(value: f32) => "flex-grow" => value.to_string();
        flex_shrink(value: f32) => "flex-shrink" => value.to_string();
    }

    fn scroll_to(
        &mut self,
        self_: Resource<Element>,
        top: u32,
        left: u32,
    ) -> wasmtime::Result<Resource<Element>> {
        let el = self.table.get_mut(&self_)?;
        el.set_style("scroll-top", top.to_string());
        el.set_style("scroll-left", left.to_string());
        return_owned_element(self, self_)
    }
}

impl ui::Host for PluginCtxV4 {
    fn render(&mut self, id: String, el: Resource<Element>) -> wasmtime::Result<()> {
        let el = take_or_clone_element(self, el)?;
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

        let _ = self.app_handle().emit(
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
        let _ = self.app_handle().emit(
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

impl ui::HostWithStore<PluginCtxV4> for PluginCtxV4 {
    async fn get_render_size(
        accessor: &Accessor<PluginCtxV4, Self>,
    ) -> wasmtime::Result<ui::RenderSize> {
        let (app_handle, plugin_name) = accessor.with(|mut access| {
            let ctx = access.get();
            (ctx.app_handle(), ctx.plugin_name().to_string())
        });
        let size = crate::api::host::v3::ui::fetch_render_size_raw(&app_handle, plugin_name).await;
        Ok(ui::RenderSize {
            width: size.0,
            height: size.1,
        })
    }
}
