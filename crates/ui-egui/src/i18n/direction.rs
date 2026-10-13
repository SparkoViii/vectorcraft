//! Writing direction of the UI, by language. egui has no built-in BiDi layout, so RTL support
//! is groundwork: the helpers here decide the direction; widgets that want to follow it call
//! [`layout`] instead of hard-coding `Layout::left_to_right` / `Layout::right_to_left`.

use super::Lang;

/// Languages written right-to-left.
pub const RTL_LANGUAGES: &[&str] = &["ar", "he", "fa", "ur"];

/// Is this language written right-to-left?
pub fn is_rtl(lang: Lang) -> bool {
    RTL_LANGUAGES.contains(&lang.code())
}

/// The egui layout for a language: left-to-right for most, right-to-left for Arabic, Hebrew, …
pub fn layout(lang: Lang, align: egui::Align) -> egui::Layout {
    if is_rtl(lang) { egui::Layout::right_to_left(align) } else { egui::Layout::left_to_right(align) }
}

/// True when the current UI language is written right-to-left.
pub fn current_is_rtl() -> bool {
    is_rtl(super::current())
}

/// The egui layout for the current UI language.
pub fn current_layout(align: egui::Align) -> egui::Layout {
    layout(super::current(), align)
}
