use crate::fit_handler::FitHandler;
use crate::fit_handler::current_fit_time_fine;

use crate::LATEST_HR;
use crate::HR_SAMPLES;

use std::thread::current;
use std::thread::{JoinHandle, sleep};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ant::channel::{RxError, RxHandler, TxError, TxHandler};
use ant::drivers::{is_ant_usb_device_from_device, UsbDriver};
use ant::messages::config::SetNetworkKey;
use ant::plus::profiles::heart_rate::{Display, DisplayConfig, Period, CommonData, MonitorTxDataPage};
use ant::router::Router;
use rusb::{Device, DeviceList};
use thingbuf::mpsc::errors::{TryRecvError, TrySendError};
use thingbuf::mpsc::{channel, Receiver, Sender};

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
    sender: Sender<T>,
}

struct RxReceiver<T> {
    receiver: Receiver<T>,
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

pub fn create_ant_thread(term: Arc<AtomicBool>, ctx: egui::Context, hr_points: Arc<Mutex<Vec<[f64; 2]>>>, start_fit_time: f64) -> JoinHandle<()> {
    std::thread::spawn(move || {
        // Heartrate variable
        let mut fit_handler = FitHandler::create_file().expect("Unable to create FIT file");

        // Try to find USB ANT sticks
        let mut devices: Vec<Device<_>> = DeviceList::new().expect("Unable to lookup usb devices").iter().filter(|x| is_ant_usb_device_from_device(x)).collect();
        if devices.is_empty() {
            panic!("No USB ANT adapters found");
        }
        // Just pick the first device we find in the list
        let device = devices.remove(0);
        let driver = UsbDriver::new(device).unwrap();
        let (channel_tx, router_rx) = channel(8);
        let (router_tx, channel_rx) = channel(8);
        let mut router = Router::new( driver, RxReceiver { receiver: router_rx}).unwrap();
        let snk = SetNetworkKey::new(0, [0xB9, 0xA5, 0x21, 0xFB, 0xBD, 0x72, 0xC3, 0x45]); // Get this from thisisant.com
        router.send(&snk).expect("failed to set network key");
        let chan = router .add_channel(TxSender { sender: router_tx }).expect("Failed to add ANT channel");
        let config = DisplayConfig { device_number: 0, device_number_extension: 0.into(), channel: chan, period: Period::FourHz, ant_plus_key_index: 0};
        
        let hr_points_cb = Arc::clone(&hr_points);

        let mut hr = Display::new( config, TxSender { sender: channel_tx }, RxReceiver { receiver: channel_rx});
        hr.set_rx_datapage_callback(Some(|x| {
            if let Ok(ref page) = x {
                LATEST_HR.store(page.common().computed_heart_rate, Ordering::Relaxed);
                HR_SAMPLES.fetch_add(1, Ordering::Relaxed);
            }
            println!("{:#?}", x);
        }));

        hr.set_rx_message_callback(Some(|x| println!("{:#?}", x)));
        hr.open();

        // last sample index
        let mut last_hr_sample = 0;

        while !term.load(Ordering::Relaxed) {
            router.process().unwrap();
            hr.process().unwrap();

            let current_hr_sample = HR_SAMPLES.load(Ordering::Relaxed);
            if current_hr_sample != last_hr_sample {
                let timestamp = current_fit_time_fine();
                let heart_rate = LATEST_HR.load(Ordering::Relaxed);
                let relative_x = (timestamp - start_fit_time).max(0.0);
                if let Ok(mut points) = hr_points_cb.lock() {
                    points.push([relative_x, heart_rate as f64]);
                }

                last_hr_sample = current_hr_sample;
                let hr_value = LATEST_HR.load(Ordering::Relaxed);
                fit_handler.update_file(timestamp, hr_value).expect("Unable to update FIT file");
                ctx.request_repaint();
            }
            sleep(Duration::from_millis(50));
        }
        println!("Exiting...");
        fit_handler.finish_file().expect("Unable to finish FIT file");

    })
}