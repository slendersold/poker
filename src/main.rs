#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bots;
mod cards;
mod game;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Office Hold'em")
            .with_inner_size([850.0, 600.0])
            .with_min_inner_size([740.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Office Hold'em",
        options,
        Box::new(|cc| Ok(Box::new(ui::PokerApp::new(cc)))),
    )
}
