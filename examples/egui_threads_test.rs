use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui;


struct MyApp {
    name: String,
    age: u32,
    heartrate: u32,
    heartrate_value_rx: Receiver<u32>,
    thread_gui_term: Arc<AtomicBool>,
}

impl MyApp {
    fn new(heartrate_value_rx: Receiver<u32>, thread_gui_term: Arc<AtomicBool>) -> Self {
        Self {
            name: "Arthur".to_owned(),
            age: 42,
            heartrate: 0,
            heartrate_value_rx,
            thread_gui_term,
        }
    }
}

impl eframe::App for MyApp {
    fn on_exit(&mut self) {
        self.thread_gui_term.store(true, Ordering::Relaxed);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            if self.thread_gui_term.load(Ordering::Relaxed) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if let Ok(msg) = self.heartrate_value_rx.try_recv() {
                println!("Received: {}", msg);
                self.heartrate = msg;
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
            ui.label(format!("Test '{}', heartrate {}", self.name, self.heartrate));
        });
    }
}

fn main() -> Result<(), Error> {
    let thread_1_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_1_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_1_term))?;
    let thread_2_term = Arc::clone(&thread_1_term);
    let thread_gui_term = Arc::clone(&thread_1_term);

    let (egui_ctx_tx, egui_ctx_rx) = std::sync::mpsc::channel::<egui::Context>();
    let (heartrate_value_tx, heartrate_value_rx) = std::sync::mpsc::channel::<u32>();

    let handle = thread::spawn(move || {
        // egui context...
        let ctx = egui_ctx_rx.recv().unwrap();
        let mut i = 0;
        while !thread_2_term.load(Ordering::Relaxed) {
            println!("thread 1 running, iteration {}", i);
            i = i + 1;
            heartrate_value_tx.send(i).map_err(|err| println!("{:?}", err)).ok();
            ctx.request_repaint();
            thread::sleep(Duration::from_millis(300));
        }
        println!("thread 1 closed");
        ctx.request_repaint();

    });

    
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





