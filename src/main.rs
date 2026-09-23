mod fit_handler;
mod ant_handler;

use ant_handler::AntHandler;

use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

use eframe::egui;
use egui_plot::{Plot, PlotPoints, PlotBounds, Line};



struct MyApp {
    name: String,
    age: u32,
    current_heartrate: u32,
    current_time: f64,
    heartrate_value_rx: Receiver<[f64; 2]>,
    thread_gui_term: Arc<AtomicBool>,
    heartrate_values: Vec<[f64; 2]>,
}

impl MyApp {
    fn new(heartrate_value_rx: Receiver<[f64; 2]>, thread_gui_term: Arc<AtomicBool>) -> Self {
        Self {
            name: "Arthur".to_owned(),
            age: 42,
            current_heartrate: 0,
            current_time: 0.0,
            heartrate_value_rx,
            thread_gui_term,
            heartrate_values: Vec::new(),
        }
    }

    fn show_plot(&mut self, ui: &mut egui::Ui) {
        let plot_points = PlotPoints::from_iter(self.heartrate_values.iter().copied());
        let line = Line::new("HR", plot_points);
        let plot = Plot::new("lines_demo");
        
        plot.show(ui, |plot_ui| {
            plot_ui.line(line);
        });
    }
}

impl eframe::App for MyApp {
    fn on_exit(&mut self) {
        // if the window is closed out then set the term to true
        self.thread_gui_term.store(true, Ordering::Relaxed);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
            ui.label(format!("Test '{}', heartrate {}", self.name, self.current_heartrate));
            ui.label(format!("Test '{}', time {}", self.name, self.current_time));

            self.show_plot(ui);
        });
    }


}

fn main() -> Result<(), Error> {
    let thread_main_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_main_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_main_term))?;
    let thread_ant_term = Arc::clone(&thread_main_term);
    let thread_gui_term = Arc::clone(&thread_main_term);

    let (egui_ctx_tx, egui_ctx_rx) = channel::<egui::Context>();
    let (heartrate_value_tx, heartrate_value_rx) = channel::<[f64; 2]>();

    let handle = thread::spawn(move || {
        let _handle = AntHandler::new()
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
            Ok(Box::<MyApp>::new(MyApp::new(heartrate_value_rx, thread_gui_term)))
        }),
    ).map_err(|err| println!("{:?}", err)).ok();
    println!("Closed egui");

    handle.join().unwrap();
    Ok(())
}