use bitcode::{Decode, Encode};
use eframe::egui::ThemePreference;

#[derive(Default, Encode, Decode, PartialEq, Clone, Copy)]
pub enum Theme {
    Dark,
    Light,
    #[default]
    System,
}

impl From<Theme> for ThemePreference {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Dark => ThemePreference::Dark,
            Theme::Light => ThemePreference::Light,
            Theme::System => ThemePreference::System,
        }
    }
}
