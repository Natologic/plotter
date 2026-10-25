
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![expect(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            Ok(Box::<MyApp>::default())
        }),
    )
}

struct MyApp {
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let text_height = 24.0;
            let avail = ui.available_size();
            ui.add_space((avail.y / 2.0) - (text_height/2.0));

            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Your Text").size(text_height));
                ui.label(egui::RichText::new("Your Text").size(text_height / 2.0));
            });
        });
    }
}