use crate::{
    app::{DATA_DIR, event::Event, events::Events, paystubs::Paystub, store::Store},
    components::Components,
};
use eframe::egui::{self, Context};
use std::{fs, path::Path};

#[derive(Default)]
pub struct Menu {}

impl Components for Menu {
    fn init(&mut self, _cc: &eframe::CreationContext<'_>, _store: &mut Store) {}

    fn update(&mut self, _context: &Context, events: &mut Events<Event>, store: &mut Store) {
        if events
            .pop(|e| match e {
                Event::ImportPaystubs => Some(()),
                _ => None,
            })
            .is_some()
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_files()
        {
            let paystubs_dir = Path::new(DATA_DIR).join("paystubs");
            if let Err(error) = fs::create_dir_all(&paystubs_dir) {
                tracing::error!(
                    caused = %error,
                    path = %paystubs_dir.display(),
                    "Création du dossier des fiches de paye."
                );
                return;
            }

            tracing::info!(count = paths.len(), "import de fiches de paie démarré");

            for path in paths {
                let Some(file_name) = path.file_name() else {
                    tracing::warn!(path = %path.display(), "Nom de fichier invalide, fiche ignorée.");
                    continue;
                };
                let destination = paystubs_dir.join(file_name);

                if destination.exists() {
                    tracing::warn!(
                        path = %destination.display(),
                        "Une fiche de paye porte déjà ce nom dans le dossier de données, fiche ignorée."
                    );
                    continue;
                }

                if let Err(error) = fs::copy(&path, &destination) {
                    tracing::error!(
                        caused = %error,
                        path = %path.display(),
                        "Copie du fichier de fiche de paye."
                    );
                    continue;
                }

                let id = uuid::Uuid::new_v4().to_string();
                let paystub = Paystub::pending(destination.to_string_lossy().to_string());
                tracing::info!(id, file = %destination.display(), "fiche de paie importée");
                store.paystubs.insert(id, paystub);
                store.save();
            }
        }
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
        events: &mut Events<Event>,
        _store: &Store,
    ) {
        egui::Panel::top("menu")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 24.0; // Espacement entre les items
                ui.horizontal(|ui| {
                    egui::Sides::new().show(
                        ui,
                        |ui| {
                            ui.label(
                                egui::RichText::new(egui_phosphor::regular::FILE_PDF).size(32.0),
                            );
                            ui.label(
                                egui::RichText::new("Analyser vos fiches de payes").size(25.0),
                            );
                        },
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(egui_phosphor::regular::GEAR)
                                            .size(20.0),
                                    )
                                    .frame(false)
                                    .min_size(egui::vec2(0.0, 40.0)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                events.push(Event::ToggleSettingsWindow { opened: true });
                            }
                            // Bouton pour importer de nouvelles fiche de paie
                            if ui
                                .scope(|ui| {
                                    ui.spacing_mut().button_padding = egui::vec2(16.0, 10.0);
                                    ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("Scanner une fiche").size(20.0),
                                        )
                                        .min_size(egui::vec2(0.0, 40.0)),
                                    )
                                })
                                .inner
                                .clicked()
                            {
                                events.push(Event::ImportPaystubs);
                            }
                        },
                    );
                });
            });
    }
}
