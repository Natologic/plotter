mod fit_handler;
mod ant_handler;

use ant_handler::AntHandler;

use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

use eframe::egui;
use egui_plot::{Plot, PlotPoints, PlotBounds, Line};



struct MyApp {
    period_ms: Arc<AtomicU32>,
    rolling_x: bool,
    rolling_x_window: f64,
    min_x_bound: f64,
    min_y_bound: f64,
    max_x_bound: f64,
    max_y_bound: f64,
    current_heartrate: u32,
    current_time: f64,
    heartrate_value_rx: Receiver<[f64; 2]>,
    thread_gui_term: Arc<AtomicBool>,
    heartrate_values: Vec<[f64; 2]>,
}

impl MyApp {
    fn new(period_ms: Arc<AtomicU32>, heartrate_value_rx: Receiver<[f64; 2]>, thread_gui_term: Arc<AtomicBool>) -> Self {
        Self {
            period_ms,
            rolling_x: true,
            rolling_x_window: 20.0,
            min_x_bound: 0.0,
            min_y_bound: 0.0,
            max_x_bound: 10.0,
            max_y_bound: 300.0,
            current_heartrate: 0,
            current_time: 0.0,
            heartrate_value_rx,
            thread_gui_term,
            heartrate_values: Vec::new(),
        }
    }
}

impl eframe::App for MyApp {
    fn on_exit(&mut self) {
        // if the window is closed out then set the term to true
        self.thread_gui_term.store(true, Ordering::Relaxed);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                egui::menu::MenuButton::new("Bounds")
                .config(egui::menu::MenuConfig::default().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                .ui(ui, |ui| {
                    ui.checkbox(&mut self.rolling_x, "Rolling X");
                    // Grey out the x controls if rolling is checked
                    ui.add_enabled_ui(self.rolling_x, |ui| {
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("Rolling X Window:");
                            let mut value = self.rolling_x_window;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap min rolling x window at something small (5)
                                self.rolling_x_window = value.max(5.0);
                            }
                        });
                    });
                    ui.add_enabled_ui(!self.rolling_x, |ui| {
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("X Min:");
                            let mut value = self.min_x_bound;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap if min value is greater than max value
                                self.min_x_bound = value.min(self.max_x_bound);
                            }
                        });
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("X Max:");
                            let mut value = self.max_x_bound;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap if max value is less than min value
                                self.max_x_bound = value.max(self.min_x_bound);
                            }
                        });
                    });
                    ui.separator();
                    // Show the y controls regardless of rolling x
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Y Min:");
                        let mut value = self.min_y_bound;
                        if ui.add(egui::DragValue::new(&mut value)).changed() {
                            // Cap if min value is greater than max value
                            self.min_y_bound = value.min(self.max_y_bound);
                        }
                    });
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Y Max:");
                        let mut value = self.max_y_bound;
                        if ui.add(egui::DragValue::new(&mut value)).changed() {
                            // Cap if max value is less than min value
                            self.max_y_bound = value.max(self.min_y_bound);
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Period:");
                        let mut period = self.period_ms.load(Ordering::Relaxed);
                        if ui.add(egui::DragValue::new(&mut period)).changed() {
                            self.period_ms.store(period, Ordering::Relaxed);
                        }
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
            if self.thread_gui_term.load(Ordering::Relaxed) {
                // if the term is already set then close the viewport
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Update the heartrate and time variables if we receive an updated heartrate
            if let Ok(msg) = self.heartrate_value_rx.try_recv() {
                println!("Received: {:?}", msg);
                self.heartrate_values.push(msg);
                self.current_heartrate = msg[1] as u32;
                self.current_time = msg[0];
            }

            ui.heading("Heartrate");

            ui.label(format!("Heartrate {}",  self.current_heartrate));
            ui.label(format!("Time {}", self.current_time));
            ui.label(format!("Period {}", self.period_ms.load(Ordering::Relaxed)));


            let plot_points = PlotPoints::from_iter(self.heartrate_values.iter().copied());
            let line = Line::new("HR", plot_points);
            let plot = Plot::new("lines_demo");

            let mut xmin = self.min_x_bound;
            let mut xmax = self.max_x_bound;
            let ymin = self.min_y_bound;
            let ymax = self.max_y_bound;
            
            plot.show(ui, |plot_ui| {
                plot_ui.line(line);
                if self.rolling_x {
                    xmin = (self.current_time - self.rolling_x_window).max(0.0);
                    xmax = self.current_time;
                }
                plot_ui.set_plot_bounds(PlotBounds::from_min_max([xmin, ymin],[xmax, ymax]));

            });

        });
    }


}

fn main() -> Result<(), Error> {
    // thread terminators
    let thread_main_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_main_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_main_term))?;
    let thread_ant_term = Arc::clone(&thread_main_term);
    let thread_gui_term = Arc::clone(&thread_main_term);

    // polling period atomic
    let thread_period_ms = Arc::new(AtomicU32::new(300));
    let clone_period_ms = Arc::clone(&thread_period_ms);

    let (egui_ctx_tx, egui_ctx_rx) = channel::<egui::Context>();
    let (heartrate_value_tx, heartrate_value_rx) = channel::<[f64; 2]>();

    let handle = thread::spawn(move || {
        let _handle = AntHandler::new(thread_period_ms)
            .expect("Cannot start ANT handler")
            .run(thread_ant_term, egui_ctx_rx, heartrate_value_tx);
    });

    // Finally run egui in the main thread
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| {
            let _ = egui_ctx_tx.send(cc.egui_ctx.clone());
            Ok(Box::<MyApp>::new(MyApp::new(clone_period_ms, heartrate_value_rx, thread_gui_term)))
        }),
    ).map_err(|err| println!("{:?}", err)).ok();
    println!("Closed egui");

    handle.join().unwrap();
    Ok(())
}