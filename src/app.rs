use crate::{
    app::{event::Event, events::Events, store::Store},
    components::{Components, graph::Graph, menu::Menu, paystubs::Paystubs, settings::Settings},
};
use eframe::{CreationContext, egui};
use tracing::level_filters::LevelFilter;

pub mod analyse;
pub mod event;
pub mod events;
pub mod paystubs;
pub mod store;
pub mod theme;

const DEFAULT_LOG_LEVEL: LevelFilter = tracing::level_filters::LevelFilter::INFO;
pub const DATA_DIR: &str = "datas";

pub struct App {
    pub events: Events<Event>,
    pub components: Vec<Box<dyn Components>>,
    pub store: Store,
}

impl App {
    pub fn load() -> App {
        // Demarrage du logger
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::builder()
                    .with_default_directive(DEFAULT_LOG_LEVEL.into())
                    .from_env_lossy(),
            )
            .init();
        // Chargement du store
        let store = Store::load()
            .map_err(|error| {
                tracing::error!(error);
                panic!("{}", error);
            })
            .unwrap();
        // Création de l'App
        App {
            events: Events::default(),
            components: vec![
                Box::new(Menu::default()),
                Box::new(Settings::default()),
                Box::new(Paystubs::default()),
                Box::new(Graph::default()),
            ],
            store,
        }
    }

    pub fn name(&self) -> &'static str {
        "Fiche de paye"
    }

    pub fn options(&self) -> eframe::NativeOptions {
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 1000.0]),
            centered: true,
            ..Default::default()
        }
    }

    pub fn init(&mut self, cc: &CreationContext<'_>) {
        // Initialisation de la font phosphor
        let mut fonts: egui::FontDefinitions = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Thin);
        cc.egui_ctx.set_fonts(fonts);

        for view in &mut self.components {
            view.init(cc, &mut self.store);
        }
    }

    pub fn tick(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        for view in &mut self.components {
            view.update(ui.ctx(), &mut self.events, &mut self.store);
        }
        for view in &mut self.components {
            view.show(ui, frame, &mut self.events, &self.store);
        }
    }

    pub fn save(&mut self) {
        self.store.save();
    }
}
