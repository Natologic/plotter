use crate::fit_handler::FitHandler;
use crate::fit_handler::current_fit_time_fine;


use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use ant::channel::{RxError, RxHandler, TxError, TxHandler};
use ant::messages::{AntMessage, TxMessage};
use ant::drivers::{is_ant_usb_device_from_device, UsbDriver};
use ant::messages::config::SetNetworkKey;
use ant::plus::profiles::heart_rate::{Display, DisplayConfig, Period, CommonData, MonitorTxDataPage};
use ant::router::Router;
use rusb::GlobalContext;
use rusb::{Device, DeviceList};
use thingbuf::mpsc::errors::{TryRecvError, TrySendError};
use thingbuf::mpsc::{channel as channel_thingbuf, Receiver as Receiver_thingbuf, Sender as Sender_thingbuf};

static CURRENT_HEARTRATE: AtomicU16 = AtomicU16::new(0);

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


pub struct AntHandler {
    fit_handler: FitHandler,
    router: Router<rusb::Error, UsbDriver<GlobalContext>, TxSender<AntMessage>, RxReceiver<TxMessage>>,
    hr: Display<TxSender<TxMessage>, RxReceiver<AntMessage>>,
    period_ms: Arc<AtomicU32>,
}

impl AntHandler {
    pub fn new(period_ms: Arc<AtomicU32>) -> Result<Self, String> {
        let fit_handler = FitHandler::create_file().expect("Unable to create FIT file");

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
        let chan = router .add_channel(TxSender { sender: router_tx }).expect("Failed to add ANT channel");
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

        Ok(Self{fit_handler, router, hr, period_ms})
    }

    pub fn run(mut self, thread_ant_term:Arc<AtomicBool>, egui_ctx_rx: Receiver<egui::Context>, heartrate_value_tx: Sender<[f64; 2]>) {
        // get the egui context so we can request to repaint it later
        let ctx = egui_ctx_rx.recv().unwrap();

        let start_fit_time = current_fit_time_fine();
        let mut last_sent = Instant::now();

        // run this thread in a loop until we get the terminator
        while !thread_ant_term.load(Ordering::Relaxed) {
            self.router.process().unwrap();
            self.hr.process().unwrap();

            if last_sent.elapsed() >= Duration::from_millis((self.period_ms.load(Ordering::Relaxed) as u64)) {
                let timestamp = current_fit_time_fine();
                let fit_offset = timestamp - start_fit_time;
                let heart_rate = CURRENT_HEARTRATE.load(Ordering::Relaxed) as f64;
                heartrate_value_tx.send([fit_offset, heart_rate]).map_err(|err| println!("{:?}", err)).ok();
                ctx.request_repaint();
                let _ = self.fit_handler.update_file(timestamp, heart_rate as u8);
                last_sent = Instant::now();
            }
        }
        println!("ANT thread termination received");
        self.fit_handler.finish_file().expect("Unable to finish FIT file");
        ctx.request_repaint();
    }

}






