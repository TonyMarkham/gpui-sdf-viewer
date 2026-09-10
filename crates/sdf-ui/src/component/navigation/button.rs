use gpui::App;
use gpui_component::ActiveTheme as _;
use gpui_component::button::ButtonCustomVariant;

pub(crate) fn variant(selected: bool, cx: &App) -> ButtonCustomVariant {
    let theme = cx.theme();

    if selected {
        ButtonCustomVariant::new(cx)
            .color(theme.sidebar_primary)
            .foreground(theme.sidebar_primary_foreground)
            .hover(theme.sidebar_primary)
            .active(theme.sidebar_primary)
            .shadow(false)
    } else {
        ButtonCustomVariant::new(cx)
            .color(theme.sidebar)
            .foreground(theme.sidebar_foreground)
            .hover(theme.sidebar_accent)
            .active(theme.sidebar_accent)
            .shadow(false)
    }
}
