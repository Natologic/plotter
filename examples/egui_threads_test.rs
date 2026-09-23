use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;

use egui_plot::{Plot, PlotPoints, PlotBounds, Line};

use rand::random_range;

use ant::channel::{RxError, RxHandler, TxError, TxHandler};
use ant::drivers::{is_ant_usb_device_from_device, UsbDriver};
use ant::messages::config::SetNetworkKey;
use ant::plus::profiles::heart_rate::{Display, DisplayConfig, Period, CommonData, MonitorTxDataPage};
use ant::router::Router;
use rusb::{Device, DeviceList};
use thingbuf::mpsc::errors::{TryRecvError, TrySendError};
use thingbuf::mpsc::{channel as channel_thingbuf, Receiver as Receiver_thingbuf, Sender as Sender_thingbuf};

static CURRENT_HEARTRATE: AtomicU16 = AtomicU16::new(0);

////////////////////
trait HasCommon {
    fn common(&self) -> &CommonData;
}

impl HasCommon for MonitorTxDataPage {
    fn common(&self) -> &CommonData {
        match self {
            MonitorTxDataPage::DefaultDataPage(p) => &p.common,
            MonitorTxDataPage::CumulativeOperatingTime(p) => &p.common,
            MonitorTxDataPage::ManufacturerInformation(p) => &p.common,
            MonitorTxDataPage::ProductInformation(p) => &p.common,
            MonitorTxDataPage::PreviousHeartBeat(p) => &p.common,
            MonitorTxDataPage::SwimIntervalSummary(p) => &p.common,
            MonitorTxDataPage::Capabilities(p) => &p.common,
            MonitorTxDataPage::BatteryStatus(p) => &p.common,
            MonitorTxDataPage::DeviceInformation(p) => &p.common,
            MonitorTxDataPage::ManufacturerSpecific(p) => &p.common,
        }
    }
}
struct TxSender<T> {
    sender: Sender_thingbuf<T>,
}

struct RxReceiver<T> {
    receiver: Receiver_thingbuf<T>,
}

impl<T: Default + Clone> TxHandler<T> for TxSender<T> {
    fn try_send(&self, msg: T) -> Result<(), TxError> {
        match self.sender.try_send(msg) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(_)) => Err(TxError::Full),
            Err(TrySendError::Closed(_)) => Err(TxError::Closed),
            Err(_) => Err(TxError::UnknownError),
        }
    }
}

impl<T: Default + Clone> RxHandler<T> for RxReceiver<T> {
    fn try_recv(&self) -> Result<T, RxError> {
        match self.receiver.try_recv() {
            Ok(e) => Ok(e),
            Err(TryRecvError::Empty) => Err(RxError::Empty),
            Err(TryRecvError::Closed) => Err(RxError::Closed),
            Err(_) => Err(RxError::UnknownError),
        }
    }
}
////////////////////////


pub fn current_fit_time_fine() -> f64 {
    // Subtract the epoch from the current time. FIT epoch starts 1989-12-31T00:00:00Z which is 631065600
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() - 631065600.0
}

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
        let mut plot = Plot::new("lines_demo");
        
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
    let thread_1_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_1_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_1_term))?;
    let thread_2_term = Arc::clone(&thread_1_term);
    let thread_gui_term = Arc::clone(&thread_1_term);

    let (egui_ctx_tx, egui_ctx_rx) = channel::<egui::Context>();
    let (heartrate_value_tx, heartrate_value_rx) = channel::<[f64; 2]>();



    let handle = thread::spawn(move || {
        // ant
        // Try to find USB ANT sticks
        let mut devices: Vec<Device<_>> = DeviceList::new().expect("Unable to lookup usb devices").iter().filter(|x| is_ant_usb_device_from_device(x)).collect();
        if devices.is_empty() {
            panic!("No USB ANT adapters found");
        }
        // Just pick the first device we find in the list
        let device = devices.remove(0);
        let driver = UsbDriver::new(device).unwrap();
        let (channel_tx, router_rx) = channel_thingbuf(8);
        let (router_tx, channel_rx) = channel_thingbuf(8);
        let mut router = Router::new( driver, RxReceiver { receiver: router_rx}).unwrap();
        let snk = SetNetworkKey::new(0, [0xB9, 0xA5, 0x21, 0xFB, 0xBD, 0x72, 0xC3, 0x45]); // Get this from thisisant.com
        router.send(&snk).expect("failed to set network key");
        let chan = router.add_channel(TxSender { sender: router_tx }).expect("Failed to add ANT channel");
        let config = DisplayConfig { device_number: 0, device_number_extension: 0.into(), channel: chan, period: Period::FourHz, ant_plus_key_index: 0};

        let mut hr = Display::new( config, TxSender { sender: channel_tx }, RxReceiver { receiver: channel_rx});
        hr.set_rx_datapage_callback(Some(|x| {
            if let Ok(ref page) = x {
                let heart_rate = page.common().computed_heart_rate;
                CURRENT_HEARTRATE.store(heart_rate as u16, Ordering::Relaxed);
            }
            println!("{:#?}", x);
        }));
        hr.set_rx_message_callback(Some(|x| println!("{:#?}", x)));
        hr.open();



        // get the egui context so we can request to repaint it later
        let ctx = egui_ctx_rx.recv().unwrap();
        // starting timestamp
        let start_fit_time = current_fit_time_fine();

        let mut last_sent = Instant::now();

        // run this thread in a loop until we get the terminator
        while !thread_2_term.load(Ordering::Relaxed) {
            router.process().unwrap();
            hr.process().unwrap();

            if last_sent.elapsed() >= Duration::from_millis(300) {
                let fit_offset = current_fit_time_fine() - start_fit_time;
                //let current_heartrate = random_range(60.0..=100.0);
                heartrate_value_tx.send([fit_offset, CURRENT_HEARTRATE.load(Ordering::Relaxed) as f64]).map_err(|err| println!("{:?}", err)).ok();
                ctx.request_repaint();
                last_sent = Instant::now();
            }
        }
        println!("thread 2 termination received");
        ctx.request_repaint();

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





