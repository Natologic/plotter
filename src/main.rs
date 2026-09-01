mod fit_handler;
mod ant_handler;

use std::io::Error;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use egui_plot::{Line, PlotBounds};
use egui_plot::Plot;
use egui_plot::PlotPoints;

use eframe::egui;

use crate::fit_handler::current_fit_time_fine;

pub struct HeartratePlot {
    x_range: f64,
    lock: bool,
    hr_points: Arc<Mutex<Vec<[f64; 2]>>>,
    term: Arc<AtomicBool>,
}

impl HeartratePlot {
    pub fn new(
        _cc: &eframe::CreationContext<'_>, hr_points: Arc<Mutex<Vec<[f64; 2]>>>, term: Arc<AtomicBool>) -> Self {
        Self {
            x_range: 20.0,
            lock: true,
            hr_points,
            term,
        }
    }

    pub fn update_plot(&mut self, ui: &mut egui::Ui) {
        // get the time
        let points = self.hr_points.lock().unwrap();
        let x_current = points.last().map(|p| p[0]).unwrap_or(0.0);

        // set the lower bound to 0 if we are less than the range
        let x_upper_bound = x_current;
        let x_lower_bound = if x_current > self.x_range {
            x_upper_bound - self.x_range
        }
        else {
            0.0
        };

        let plot_points = PlotPoints::from_iter(points.iter().copied());
        let line = Line::new("HR", plot_points);

        Plot::new("HR")
            .view_aspect(2.0)
            .show(ui, |plot_ui| {
                if self.lock == true {
                    plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                        [x_lower_bound, 40.0],
                        [x_upper_bound, 200.0]
                    ));
                }
                plot_ui.line(line);
            });
        
    }
}

impl eframe::App for HeartratePlot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.term.load(Ordering::Relaxed) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.update_plot(ui);
    }

    fn on_exit(&mut self) {
        self.term.store(true, Ordering::Relaxed);
    }

}

fn main() -> Result<(), Error> {
    // Register signal hook and clone it for closing the plot window
    let term = Arc::new(AtomicBool::new(false));
    let app_term = Arc::clone(&term);
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;
    // initalize the plot
    let start_fit_time = current_fit_time_fine();
    let hr_points = Arc::new(Mutex::new(Vec::new()));
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Heartrate",
        native_options,
        Box::new(|cc| {
            ant_handler::create_ant_thread(
                app_term,
                cc.egui_ctx.clone(),
                Arc::clone(&hr_points),
                start_fit_time,
            );
            Ok(Box::new(HeartratePlot::new(cc, hr_points, term)))    
        }),
    ).map_err(std::io::Error::other)?;


    Ok(())
}