use std::{
    collections::{BTreeMap, HashSet},
    fmt::format,
};

use eframe::egui::{self, Color32, Popup};
use time::Date;
use tracing::{debug, info};

use crate::{
    app::{paystubs::PaystubState, store::Store},
    components::Components,
};

/// Clé de filtre virtuelle pour `net_salary` : ce n'est pas une entrée de
/// `datas` (c'est un champ dédié de `PaystubState::Completed`, garanti sur
/// toute fiche), mais on veut pouvoir la sélectionner et l'afficher comme
/// n'importe quel autre filtre du graphique.
const NET_SALARY_KEY: &str = "Salaire net";

/// Noms de mois abrégés en français, indexés de 0 (janvier) à 11 (décembre) ;
/// voir `month_bucket` / `month_bucket_label`.
const MONTH_NAMES_FR: [&str; 12] = [
    "Janv.", "Févr.", "Mars", "Avr.", "Mai", "Juin", "Juil.", "Août", "Sept.", "Oct.", "Nov.",
    "Déc.",
];

/// Convertit une date en un indice de mois croissant et régulier
/// (`année * 12 + mois-1`), pour espacer les barres uniformément même si les
/// dates de paiement réelles ne tombent pas à intervalles constants.
fn month_bucket(date: Date) -> i32 {
    date.year() * 12 + i32::from(u8::from(date.month())) - 1
}

fn month_bucket_label(bucket: i32) -> String {
    let year = bucket.div_euclid(12);
    let month = bucket.rem_euclid(12) as usize;
    format!("{} {year}", MONTH_NAMES_FR[month])
}

/// Barres `(mois, valeur)` de la clé `key`, une par mois calendaire, pour
/// toutes les fiches `Completed` du store où cette clé est présente. Si
/// plusieurs fiches tombent dans le même mois, leurs valeurs sont sommées.
/// `payment_date` est garanti par le prompt système de `Analyse` (voir
/// `PaystubState::Completed`), plus besoin de le deviner ici. `key ==
/// NET_SALARY_KEY` lit `net_salary` directement plutôt que `datas`.
fn monthly_bars(store: &Store, key: &str) -> Vec<(i32, f64)> {
    let mut buckets: BTreeMap<i32, f64> = BTreeMap::new();

    for paystub in store.paystubs.values() {
        let PaystubState::Completed {
            payment_date,
            net_salary,
            datas,
            ..
        } = &paystub.state
        else {
            continue;
        };
        let Some(value) = (if key == NET_SALARY_KEY {
            Some(*net_salary)
        } else {
            datas.get(key).copied()
        }) else {
            continue;
        };
        let Ok(date) = Date::from_julian_day(*payment_date) else {
            continue;
        };
        *buckets.entry(month_bucket(date)).or_insert(0.0) += f64::from(value);
    }

    debug!(
        key,
        count = buckets.len(),
        "barres mensuelles calculées pour le graphique"
    );

    buckets.into_iter().collect()
}

pub struct Graph {
    filters: Vec<String>,
    /// Cache des clés uniques présentes dans les `datas` de toutes les
    /// fiches `Completed` du store, triées alphabétiquement.
    datas_keys_cache: Vec<String>,
    /// Nombre de fiches `Completed` lors du dernier calcul de
    /// `datas_keys_cache` : signal peu coûteux pour savoir si le cache est
    /// encore à jour, sans reconstruire la liste des clés à chaque frame.
    cached_completed_count: usize,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            // "Salaire net" est sélectionné par défaut au lancement : c'est
            // la donnée la plus universellement pertinente d'une fiche de
            // paie, contrairement aux entrées de `datas` qui varient d'un
            // employeur à l'autre.
            filters: vec![NET_SALARY_KEY.to_string()],
            datas_keys_cache: Vec::new(),
            cached_completed_count: 0,
        }
    }
}

/// Compte les fiches `Completed` du store ; sert de signal d'invalidation
/// pour `datas_keys_cache`, beaucoup moins coûteux que reconstruire et
/// retrier l'ensemble des clés à chaque frame.
fn count_completed(store: &Store) -> usize {
    store
        .paystubs
        .values()
        .filter(|paystub| matches!(paystub.state, PaystubState::Completed { .. }))
        .count()
}

/// Collecte, dédoublonne et trie les clés de `datas` de toutes les fiches
/// `Completed` du store.
fn collect_datas_keys(store: &Store) -> Vec<String> {
    let mut keys = HashSet::new();
    keys.insert(NET_SALARY_KEY);
    for paystub in store.paystubs.values() {
        if let PaystubState::Completed { datas, .. } = &paystub.state {
            keys.extend(datas.keys().map(String::as_str));
        }
    }
    let mut keys: Vec<String> = keys.into_iter().map(str::to_string).collect();
    keys.sort_unstable();
    keys
}

impl Components for Graph {
    fn init(&mut self, _cc: &eframe::CreationContext<'_>, _store: &mut crate::app::store::Store) {}

    fn update(
        &mut self,
        _context: &eframe::egui::Context,
        _events: &mut crate::app::events::Events<crate::app::event::Event>,
        _store: &mut crate::app::store::Store,
    ) {
    }

    fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        _frame: &mut eframe::Frame,
        _events: &mut crate::app::events::Events<crate::app::event::Event>,
        store: &crate::app::store::Store,
    ) {
        let completed_count = count_completed(store);
        if completed_count != self.cached_completed_count {
            self.datas_keys_cache = collect_datas_keys(store);
            self.cached_completed_count = completed_count;
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let response = ui
                .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                    ui.style_mut().interaction.selectable_labels = false;
                    let hovered = ui.response().hovered();
                    egui::Frame::group(ui.style())
                        .fill(if hovered {
                            ui.visuals().widgets.hovered.weak_bg_fill
                        } else {
                            Color32::TRANSPARENT
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.set_width(300.0);
                                egui::Sides::new().show(
                                    ui,
                                    |ui| ui.label(egui::RichText::new("Filtres").size(15.0)),
                                    |ui| {
                                        egui::Frame::new()
                                            .fill(ui.visuals().widgets.inactive.bg_fill) // gris qui contraste avec le fond
                                            .corner_radius(egui::CornerRadius::same(8))
                                            .inner_margin(egui::Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{}",
                                                        self.filters.len(),
                                                    ))
                                                    .size(12.0)
                                                    .color(ui.visuals().weak_text_color()), // gris pour le texte
                                                );
                                            })
                                    },
                                );
                            });
                        });
                })
                .response;

            Popup::menu(&response)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_width(300.0);
                    let max_height = (ui.ctx().viewport_rect().height() * 0.6).min(300.0);
                    ui.set_max_height(max_height);
                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .show(ui, |ui| {
                            for key in &self.datas_keys_cache {
                                let mut checked = self.filters.contains(key);
                                if ui.checkbox(&mut checked, key).clicked() {
                                    if checked {
                                        self.filters.push(key.clone());
                                    } else {
                                        self.filters.retain(|f| f != key);
                                    }
                                }
                            }
                        });
                });

            ui.add_space(20.0);

            egui_plot::Plot::new("paystubs_graph")
                .legend(egui_plot::Legend::default())
                .x_axis_formatter(|mark, _range| month_bucket_label(mark.value.round() as i32))
                .show(ui, |plot_ui| {
                    // Barres groupées côte à côte par mois : chaque filtre
                    // actif se voit attribuer une tranche de la largeur d'un
                    // mois, centrée sur celui-ci, plutôt que de superposer
                    // les barres de chaque filtre les unes sur les autres.
                    let filter_count = self.filters.len().max(1) as f64;
                    let group_width = 0.8;
                    let bar_width = group_width / filter_count;

                    for (i, key) in self.filters.iter().enumerate() {
                        let offset = (i as f64 - (filter_count - 1.0) / 2.0) * bar_width;
                        let bars: Vec<egui_plot::Bar> = monthly_bars(store, key)
                            .into_iter()
                            .map(|(bucket, value)| {
                                egui_plot::Bar::new(f64::from(bucket) + offset, value)
                            })
                            .collect();
                        if !bars.is_empty() {
                            plot_ui.bar_chart(
                                egui_plot::BarChart::new(key.clone(), bars).width(bar_width),
                            );
                        }
                    }
                });
        });
    }
}
