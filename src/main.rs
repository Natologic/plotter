mod fit_handler;
mod ant_handler;

use ant::messages::config;
use ant_handler::AntHandler;

use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::thread;


use eframe::egui;
use egui_plot::{Plot, PlotPoints, PlotBounds, Line};

use windows_sys::Win32::System::Console::{GetStdHandle, GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_QUICK_EDIT_MODE, STD_INPUT_HANDLE};

use serde::{Serialize, Deserialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    period_ms: u32,
    rolling_x: bool,
    rolling_x_window: f64,
    min_x_bound: f64,
    min_y_bound: f64,
    max_x_bound: f64,
    max_y_bound: f64,
    window_width: f32,
    window_height: f32,
    display_hrm: bool,
    display_time: bool,
}

impl Default for Config{
    fn default() -> Self {
        Self {
            period_ms: 300,
            rolling_x: true,
            rolling_x_window: 60.0,
            min_x_bound: 0.0,
            min_y_bound: 0.0,
            max_x_bound: 10.0,
            max_y_bound: 200.0,
            window_width: 400.0,
            window_height: 300.0,
            display_hrm: true,
            display_time: true,
        }
    }
}

struct MyApp {
    period_ms: Arc<AtomicU32>,
    current_heartrate: u32,
    current_time: f64,
    heartrate_value_rx: Receiver<[f64; 2]>,
    thread_gui_term: Arc<AtomicBool>,
    heartrate_values: Vec<[f64; 2]>,
    config: Config,
    config_path: std::path::PathBuf,
}

impl MyApp {
    fn new(period_ms: Arc<AtomicU32>, heartrate_value_rx: Receiver<[f64; 2]>, thread_gui_term: Arc<AtomicBool>, config: Config, config_path: std::path::PathBuf) -> Self {
        Self {
            period_ms,
            current_heartrate: 0,
            current_time: 0.0,
            heartrate_value_rx,
            thread_gui_term,
            heartrate_values: Vec::new(),
            config,
            config_path,
        }
    }

    fn save_config(&self) {
        match serde_json::to_string_pretty(&self.config) {
            Ok(json) => {
                match std::fs::write(&self.config_path, json) {
                    Ok(_) => println!("Successfully saved config to: {:?}", self.config_path),
                    Err(e) => eprintln!("FAILED to write config file: {:?}", e),
                }
            }
            Err(e) => eprintln!("FAILED to serialize config: {:?}", e),
        }
    }
}

impl eframe::App for MyApp {
    fn on_exit(&mut self) {
        self.save_config();
        // if the window is closed out then set the term to true
        self.thread_gui_term.store(true, Ordering::Relaxed);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // keep track of window size
        let size = ui.ctx().content_rect();
        self.config.window_width = size.width();
        self.config.window_height = size.height();

        ui.ctx().set_visuals(egui::Visuals::dark());

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                egui::menu::MenuButton::new("Bounds")
                .config(egui::menu::MenuConfig::default().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside))
                .ui(ui, |ui| {
                    ui.checkbox(&mut self.config.rolling_x, "Rolling X");
                    // Grey out the x controls if rolling is checked
                    ui.add_enabled_ui(self.config.rolling_x, |ui| {
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("Rolling X Window:");
                            let mut value = self.config.rolling_x_window;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap min rolling x window at something small (5)
                                self.config.rolling_x_window = value.max(5.0);
                            }
                        });
                    });
                    ui.add_enabled_ui(!self.config.rolling_x, |ui| {
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("X Min:");
                            let mut value = self.config.min_x_bound;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap if min value is greater than max value
                                self.config.min_x_bound = value.min(self.config.max_x_bound);
                            }
                        });
                        ui.horizontal(|ui: &mut egui::Ui| {
                            ui.label("X Max:");
                            let mut value = self.config.max_x_bound;
                            if ui.add(egui::DragValue::new(&mut value)).changed() {
                                // Cap if max value is less than min value
                                self.config.max_x_bound = value.max(self.config.min_x_bound);
                            }
                        });
                    });
                    ui.separator();
                    // Show the y controls regardless of rolling x
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Y Min:");
                        let mut value = self.config.min_y_bound;
                        if ui.add(egui::DragValue::new(&mut value)).changed() {
                            // Cap if min value is greater than max value
                            self.config.min_y_bound = value.min(self.config.max_y_bound);
                        }
                    });
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Y Max:");
                        let mut value = self.config.max_y_bound;
                        if ui.add(egui::DragValue::new(&mut value)).changed() {
                            // Cap if max value is less than min value
                            self.config.max_y_bound = value.max(self.config.min_y_bound);
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui: &mut egui::Ui| {
                        ui.label("Period:");
                        let mut period = self.period_ms.load(Ordering::Relaxed);
                        if ui.add(egui::DragValue::new(&mut period)).changed() {
                            self.period_ms.store(period, Ordering::Relaxed);
                            self.config.period_ms = period;
                        }
                    });
                });
                ui.menu_button("Displays", |ui| {
                    ui.checkbox(&mut self.config.display_hrm, "Display HR");
                    ui.checkbox(&mut self.config.display_time, "Display Timestamp");
                })
            });

        });

        egui::CentralPanel::default()
        .show(ui, |ui| {
            if self.thread_gui_term.load(Ordering::Relaxed) {
                // if the term is already set then close the viewport
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Update the heartrate and time variables if we receive an updated heartrate
            if let Ok(msg) = self.heartrate_value_rx.try_recv() {
                // println!("Received: {:?}", msg);
                self.heartrate_values.push(msg);
                self.current_heartrate = msg[1] as u32;
                self.current_time = msg[0];
            }

            let plot_points = PlotPoints::from_iter(self.heartrate_values.iter().copied());
            let line = Line::new("HR", plot_points).width(3.0).color(egui::Color32::BLUE);
            let plot = Plot::new("lines_demo")
                .x_axis_label("Time [s]")
                .y_axis_label("Heartrate [bpm]");

            let mut xmin = self.config.min_x_bound;
            let mut xmax = self.config.max_x_bound;
            let ymin = self.config.min_y_bound;
            let ymax = self.config.max_y_bound;

            ui.horizontal_centered(|ui| {
                let available_size = ui.available_size();
                if self.config.display_hrm || self.config.display_time {
                    ui.allocate_ui(egui::vec2(300.0, available_size.y), |ui| {
                        ui.vertical_centered(|ui| {
                            let hrm_height = 60.0;
                            let avail = ui.available_size();
                            ui.add_space((avail.y / 2.0) - (hrm_height/2.0));
                            if self.config.display_hrm {
                                ui.label(
                                    egui::RichText::new(format!("{}bpm", self.current_heartrate))
                                        .size(hrm_height)
                                        .strong()
                                        .color(egui::Color32::RED)
                                );
                            }
                            if self.config.display_time {
                                ui.label(
                                    egui::RichText::new(format!("{}s", self.current_time.round() as i32))
                                        .size(hrm_height/2.0)
                                        .strong()
                                        .color(egui::Color32::WHITE)
                                );
                            }
                        });
                    });

                }

                ui.allocate_ui(egui::vec2(ui.available_width(), available_size.y), |ui| {
                    plot.show(ui, |plot_ui| {
                        plot_ui.line(line);
                        if self.config.rolling_x {
                            xmin = (self.current_time - self.config.rolling_x_window).max(0.0);
                            xmax = self.current_time;
                        }
                        plot_ui.set_plot_bounds(PlotBounds::from_min_max([xmin, ymin],[xmax, ymax]));

                    });
                });
            });

        });
    }


}

#[cfg(windows)]
fn windows_disable_quickedit() {
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode: CONSOLE_MODE = 0;
        let current_mode = GetConsoleMode(handle, &mut mode);
        if current_mode != 0 {
            use windows_sys::Win32::System::Console::ENABLE_EXTENDED_FLAGS;
            mode &= !ENABLE_QUICK_EDIT_MODE;
            mode |= ENABLE_EXTENDED_FLAGS;
            SetConsoleMode(handle, mode);
        }
    }
}
fn main() -> Result<(), Error> {
    // immediately disable quickedit so clicking into the window doesn't freeze the thread
    #[cfg(windows)]
    windows_disable_quickedit();

    // config file
    let mut config_path = std::env::current_exe()?;
    config_path.pop();
    config_path.push("config.json");
    // create one if it doesn't exist
    if !config_path.exists() {
        let config_default = Config::default();
        std::fs::write(&config_path, serde_json::to_string_pretty(&config_default).unwrap())?;
        println!("Default JSON config created at: {:?}", config_path);
    }
    let config: Config = serde_json::from_str(&std::fs::read_to_string(&config_path)?).unwrap();

    // thread terminators
    let thread_main_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_main_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_main_term))?;
    let thread_ant_term = Arc::clone(&thread_main_term);
    let thread_gui_term = Arc::clone(&thread_main_term);

    // polling period atomic
    let thread_period_ms = Arc::new(AtomicU32::new(config.period_ms));
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
        viewport: egui::ViewportBuilder::default().with_inner_size([config.window_width, config.window_height]),
        ..Default::default()
    };
    eframe::run_native(
        "Heartrate Monitor",
        options,
        Box::new(|cc| {
            let _ = egui_ctx_tx.send(cc.egui_ctx.clone());
            Ok(Box::<MyApp>::new(MyApp::new(clone_period_ms, heartrate_value_rx, thread_gui_term, config, config_path.clone())))
        }),
    ).map_err(|err| println!("{:?}", err)).ok();
    println!("Closed egui");

    handle.join().unwrap();
    Ok(())
}