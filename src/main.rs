mod fit_handler;
mod ant_handler;

use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use egui_plot::Line;
use egui_plot::Plot;
use egui_plot::PlotPoints;

use eframe::egui;

pub static LATEST_HR: AtomicU8 = AtomicU8::new(0);

pub struct HeartratePlot {
    hr_points: Vec<[f64; 2]>,
    term: Arc<AtomicBool>,
}

impl HeartratePlot {
    pub fn new(_cc: &eframe::CreationContext<'_>, term: Arc<AtomicBool>) -> Self {
        let hr_points = Vec::new();

        Self { hr_points, term }
    }

    pub fn update_plot(&mut self, ui: &mut egui::Ui) {
        let hr = LATEST_HR.load(Ordering::Relaxed) as f64;
        let x = self.hr_points.len() as f64;
        self.hr_points.push([x, hr]);
        let plot_points = PlotPoints::from(self.hr_points.clone());
        let line = Line::new("HR", plot_points); 
        Plot::new("HR").view_aspect(2.0).show(ui, |plot_ui| plot_ui.line(line));
    }
}

impl eframe::App for HeartratePlot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.term.load(Ordering::Relaxed) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.update_plot(ui);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn main() -> Result<(), Error> {
    // Register signal hook and clone it for closing the plot window
    let term = Arc::new(AtomicBool::new(false));
    let app_term = Arc::clone(&term);
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;
    // create the ANT thread
    let ant_handle = ant_handler::create_ant_thread(Arc::clone(&term));
    // initalize the plot
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Heartrate",
        native_options,
        Box::new(|cc| Ok(Box::new(HeartratePlot::new(cc, app_term)))),
    ).map_err(std::io::Error::other)?;

    term.store(true, Ordering::Relaxed);
    if let Err(e) = ant_handle.join() {
        eprintln!("ANT thread error: {:?}", e);
    }

    Ok(())
}