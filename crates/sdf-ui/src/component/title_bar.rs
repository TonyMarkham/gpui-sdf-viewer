use gpui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce,
    StatefulInteractiveElement as _, Styled, Window, WindowControlArea, div, px,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};

/// Height of the OS top resize band this title bar leaves uncovered, so the
/// window can be resized from its top edge. Must be at least the system frame
/// thickness (`SM_CYFRAME`), which the platform hit test uses for `HTTOP`.
const TOP_RESIZE_BAND: Pixels = px(8.0);
const TITLE_BAR_HEIGHT: Pixels = px(34.0);
const CONTROL_WIDTH: Pixels = px(34.0);
const LEFT_PADDING: Pixels = px(12.0);

/// The application title bar.
///
/// Unlike the gpui-component `TitleBar`, the caption drag hitbox starts below
/// a reserved top strip, so the window's top-edge resize zone is never
/// shadowed by the drag hitbox.
#[derive(IntoElement)]
pub(crate) struct TitleBar {
    children: Vec<AnyElement>,
}

impl TitleBar {
    pub(crate) fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_maximized = window.is_maximized();
        let is_linux = cfg!(target_os = "linux");

        let bar_background = cx.theme().title_bar;
        let bar_border = cx.theme().title_bar_border;

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(bar_border)
                .bg(bar_background)
                // Left uncovered for the OS top-edge resize zone.
                .child(div().h(TOP_RESIZE_BAND))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .h(TITLE_BAR_HEIGHT - TOP_RESIZE_BAND)
                        .pl(LEFT_PADDING)
                        .child(
                            h_flex()
                                .id("title-bar-caption")
                                .window_control_area(WindowControlArea::Drag)
                                .h_full()
                                .flex_1()
                                .overflow_hidden()
                                .children(self.children),
                        )
                        .child(controls(is_maximized, is_linux, cx)),
                ),
        )
    }
}

fn controls(is_maximized: bool, is_linux: bool, cx: &App) -> impl IntoElement {
    h_flex()
        .id("window-controls")
        .items_center()
        .flex_shrink_0()
        .h_full()
        .child(control_button(
            "minimize",
            IconName::WindowMinimize,
            WindowControlArea::Min,
            false,
            is_linux,
            cx,
        ))
        .child(if is_maximized {
            control_button(
                "maximize",
                IconName::WindowRestore,
                WindowControlArea::Max,
                false,
                is_linux,
                cx,
            )
        } else {
            control_button(
                "maximize",
                IconName::WindowMaximize,
                WindowControlArea::Max,
                false,
                is_linux,
                cx,
            )
        })
        .child(control_button(
            "close",
            IconName::WindowClose,
            WindowControlArea::Close,
            true,
            is_linux,
            cx,
        ))
}

fn control_button(
    id: &'static str,
    icon: IconName,
    area: WindowControlArea,
    is_close: bool,
    is_linux: bool,
    cx: &App,
) -> impl IntoElement {
    let (hover_foreground, hover_background, active_foreground, active_background) = if is_close {
        (
            cx.theme().danger_foreground,
            cx.theme().danger,
            cx.theme().danger_foreground,
            cx.theme().danger_active,
        )
    } else {
        (
            cx.theme().secondary_foreground,
            cx.theme().secondary_hover,
            cx.theme().secondary_foreground,
            cx.theme().secondary_active,
        )
    };

    let mut button = div()
        .id(id)
        .window_control_area(area)
        .flex()
        .w(CONTROL_WIDTH)
        .h_full()
        .flex_shrink_0()
        .justify_center()
        .content_center()
        .items_center()
        .text_color(cx.theme().foreground)
        .hover(move |style| style.bg(hover_background).text_color(hover_foreground))
        .active(move |style| style.bg(active_background).text_color(active_foreground))
        .child(Icon::new(icon).small());

    if is_linux {
        button = button.on_click(move |_, window, cx| {
            cx.stop_propagation();
            match area {
                WindowControlArea::Min => window.minimize_window(),
                WindowControlArea::Max => window.zoom_window(),
                _ => window.remove_window(),
            }
        });
    }

    button
}
