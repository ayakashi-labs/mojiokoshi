#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod export;
mod transcription;

use app::MeetingMinutesApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "mojiokoshi",
        native_options,
        Box::new(|cc| Ok(Box::new(MeetingMinutesApp::new(cc)))),
    )
}
