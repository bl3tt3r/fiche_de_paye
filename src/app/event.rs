use crate::app::theme::Theme;

pub enum Event {
    ToggleSettingsWindow { opened: bool },
    ChangeTheme(Theme),
    ImportPaystubs,
}
