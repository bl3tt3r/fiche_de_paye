use crate::{
    app::{event::Event, events::Events, store::Store, theme::Theme},
    components::Components,
};
use eframe::egui::{self, Context, Modal};

#[derive(Default)]
pub struct Settings {
    open: bool,
}

impl Components for Settings {
    fn init(&mut self, cc: &eframe::CreationContext<'_>, store: &mut Store) {
        cc.egui_ctx.set_theme(store.theme);
    }

    fn update(&mut self, context: &Context, events: &mut Events<Event>, store: &mut Store) {
        if let Some(opened) = events.pop(|e| match e {
            Event::ToggleSettingsWindow { opened } => Some(*opened),
            _ => None,
        }) {
            self.open = opened;
        }
        if let Some(theme) = events.pop(|e| match e {
            Event::ChangeTheme(theme) => Some(*theme),
            _ => None,
        }) {
            context.set_theme(theme);
            store.theme = theme;
            store.save();
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
        events: &mut Events<Event>,
        store: &Store,
    ) {
        if self.open {
            let modal = Modal::new("Settings".into()).show(ui.ctx(), |ui| {
                ui.set_width(250.0);
                ui.heading("Parametres");

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |ui| ui.label("Theme : "),
                    |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    store.theme == Theme::Dark,
                                    egui_phosphor::regular::MOON,
                                )
                                .clicked()
                            {
                                events.push(Event::ChangeTheme(Theme::Dark));
                            };
                            if ui
                                .selectable_label(
                                    store.theme == Theme::Light,
                                    egui_phosphor::regular::SUN,
                                )
                                .clicked()
                            {
                                events.push(Event::ChangeTheme(Theme::Light));
                            };
                            if ui
                                .selectable_label(
                                    store.theme == Theme::System,
                                    egui_phosphor::regular::COMPUTER_TOWER,
                                )
                                .clicked()
                            {
                                events.push(Event::ChangeTheme(Theme::System));
                            };
                        });
                    },
                );

                ui.separator();

                egui::Sides::new().show(
                    ui,
                    |_ui| {},
                    |ui| {
                        if ui.button("close").clicked() {
                            ui.close();
                        }
                    },
                );
            });

            if modal.should_close() {
                events.push(Event::ToggleSettingsWindow { opened: false });
            }
        }
    }
}
