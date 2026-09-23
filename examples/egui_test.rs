
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
    name: String,
    age: u32,
    period_ms: u32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            name: "Arthur".to_owned(),
            age: 42,
            period_ms: 300,
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                egui::menu::MenuButton::new("File")
                    .config(egui::menu::MenuConfig::default().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                    .ui(ui, |ui| {
                    if ui.button("1").clicked() {
                        println!("1 clicked");
                        ui.close();
                    }
                    if ui.button("2").clicked() {
                        println!("2 clicked");
                        ui.close();
                    }
                    ui.separator();
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Period:");
                        ui.add(egui::DragValue::new(&mut self.period_ms))
                    });
                });
                ui.menu_button("New", |ui| {
                    if ui.button("1").clicked() {
                        println!("1 clicked");
                        ui.close();
                    }
                    if ui.button("2").clicked() {
                        println!("2 clicked");
                        ui.close();
                    }
                })
            });

        });


        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("My egui Application");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut self.name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut self.age, 0..=120).text("age"));
            if ui.button("Increment").clicked() {
                self.age += 1;
            }
            ui.label(format!("Hello '{}', age {}", self.name, self.age));
            ui.label(format!("Period {}", self.period_ms));

        });
    }
}