mod fit_handler;
mod ant_handler;
mod config;

use ant_handler::{AntHandler, AntStatus, Outcome};
use fit_handler::FitHandler;
use fit_handler::current_fit_time_fine;
use config::Config;

use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender, Receiver, RecvTimeoutError};
use std::time::Duration;
use std::thread;

use eframe::egui;
use egui_plot::{Plot, PlotPoints, PlotBounds, Line};

use windows_sys::Win32::System::Console::{GetStdHandle, GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_QUICK_EDIT_MODE, STD_INPUT_HANDLE};


struct MyApp {
    period_ms: Arc<AtomicU32>,
    current_heartrate: u32,
    current_time: f64,
    heartrate_value_rx: Receiver<[f64; 2]>,
    thread_gui_term: Arc<AtomicBool>,
    heartrate_values: Vec<[f64; 2]>,
    config: Config,
    config_path: std::path::PathBuf,
    ant_status: Arc<AtomicU8>,
    ant_retry_tx: Sender<()>,
}

impl MyApp {
    fn new(period_ms: Arc<AtomicU32>, heartrate_value_rx: Receiver<[f64; 2]>, thread_gui_term: Arc<AtomicBool>, config: Config, config_path: std::path::PathBuf, ant_status: Arc<AtomicU8>, ant_retry_tx: Sender<()>) -> Self {
        Self {
            period_ms,
            current_heartrate: 0,
            current_time: 0.0,
            heartrate_value_rx,
            thread_gui_term,
            heartrate_values: Vec::new(),
            config,
            config_path,
            ant_status,
            ant_retry_tx,
        }
    }

}

impl eframe::App for MyApp {
    fn on_exit(&mut self) {
        if let Err(e) = self.config.save_config(&self.config_path) {
            eprintln!("Could not save config file: {e}")
        };
        // if the window is closed out then set the term to true
        self.thread_gui_term.store(true, Ordering::Relaxed);
        let _ = self.ant_retry_tx.send(());
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
                    ui.checkbox(&mut self.config.display_plot, "Display Plot");
                });
                ui.menu_button("ANT", |ui| {
                    if ui.button("Reconnect to ANT stick").clicked() {
                        let _ = self.ant_retry_tx.send(());
                    }
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
                // Numeric readouts for HRM and time duration
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
                // Plot
                if self.config.display_plot {
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
                }
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

    // load the configuration
    let config_path = Config::default_path()?;
    let config = Config::load_config(&config_path)?;

    // thread terminators
    let thread_main_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_main_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_main_term))?;
    let thread_ant_term = Arc::clone(&thread_main_term);
    let thread_gui_term = Arc::clone(&thread_main_term);

    // polling period atomic
    let thread_period_ms = Arc::new(AtomicU32::new(config.period_ms));
    let clone_period_ms = Arc::clone(&thread_period_ms);
    // ant status atomic
    let ant_status = Arc::new(AtomicU8::new(AntStatus::NoDevice as u8));
    let clone_ant_status = Arc::clone(&ant_status);
    // ant retry 
    let (ant_retry_tx, ant_retry_rx) = channel::<()>();

    let (egui_ctx_tx, egui_ctx_rx) = channel::<egui::Context>();
    let (heartrate_value_tx, heartrate_value_rx) = channel::<[f64; 2]>();

    let handle = thread::spawn(move || {
        // get the egui context so the thread can request repaint later
        let ctx = egui_ctx_rx.recv().unwrap();

        // create fit file
        let start_fit_time = current_fit_time_fine();
        let mut fit_handler = FitHandler::create_file().expect("Unable to create FIT file");

        let mut retry_connection = true;
        loop {
            if retry_connection {
                retry_connection = false;
                // try to start an ant thread
                match AntHandler::new(Arc::clone(&thread_period_ms), Arc::clone(&clone_ant_status)) {
                    Err(e) => eprintln!("Could not start ANT thread: {e}"),
                    Ok(x) => {
                        // run the thread, if it terminates then break, if it fails loop back around
                        match x.run(Arc::clone(&thread_ant_term), ctx.clone(), heartrate_value_tx.clone(), Arc::clone(&clone_ant_status), &mut fit_handler, start_fit_time) {
                            Outcome::Terminated => break,
                            Outcome::Failed => {}
                        }
                    } 
                }
            }
            // block until the retry button is pressed or we time out
            match ant_retry_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(()) => {
                    // check first if the term was sent, if so, quit out of loop
                    if thread_ant_term.load(Ordering::Relaxed) {
                        break;
                    }
                    // otherwise good to re-loop and try to restart the thread again
                    retry_connection = true;
                }
                Err(RecvTimeoutError::Timeout) => {
                    // if the timeout happened and the term is sent, quit out of the loop
                    if thread_ant_term.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        // finish fit file
        fit_handler.finish_file().expect("Unable to finish FIT file");
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
            Ok(Box::<MyApp>::new(MyApp::new(clone_period_ms, heartrate_value_rx, thread_gui_term, config, config_path.clone(), ant_status, ant_retry_tx)))
        }),
    ).map_err(|err| println!("{:?}", err)).ok();
    println!("Closed egui");

    handle.join().unwrap();
    Ok(())
}