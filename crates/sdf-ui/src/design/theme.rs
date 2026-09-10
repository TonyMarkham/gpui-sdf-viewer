use crate::{UiError, UiResult, constants::BLUEPRINT_FUNCTIONAL_THEME_JSON};

use gpui_component::{ThemeConfig, ThemeSet};
use std::rc::Rc;

pub(crate) fn load() -> UiResult<Rc<ThemeConfig>> {
    let theme_set: ThemeSet =
        serde_json::from_str(BLUEPRINT_FUNCTIONAL_THEME_JSON).map_err(UiError::theme)?;

    let theme = theme_set
        .themes
        .into_iter()
        .next()
        .ok_or_else(UiError::theme_missing)?;

    Ok(Rc::new(theme))
}
