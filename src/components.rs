use eframe::{
    CreationContext,
    egui::{self, Context},
};

use crate::app::{event::Event, events::Events, store::Store};

pub mod graph;
pub mod menu;
pub mod paystubs;
pub mod settings;

pub trait Components {
    fn init(&mut self, _cc: &CreationContext<'_>, _store: &mut Store) {}
    fn update(&mut self, _context: &Context, _events: &mut Events<Event>, _store: &mut Store) {}
    fn show(
        &mut self,
        _ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
        _events: &mut Events<Event>,
        _store: &Store,
    ) {
    }
}
