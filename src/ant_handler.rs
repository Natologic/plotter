use crate::fit_handler::FitHandler;
use crate::fit_handler::current_fit_time_fine;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use ant::channel::{RxError, RxHandler, TxError, TxHandler};
use ant::messages::{AntMessage, TxMessage};
use ant::drivers::{is_ant_usb_device_from_device, UsbDriver};
use ant::messages::config::{SetNetworkKey,SearchTimeout};
use ant::plus::profiles::heart_rate::{Display, DisplayConfig, Period, CommonData, MonitorTxDataPage};
use ant::router::Router;
use ant::router::RouterError;
use rusb::GlobalContext;
use rusb::{Device, DeviceList};
use thingbuf::mpsc::errors::{TryRecvError, TrySendError};
use thingbuf::mpsc::{channel as channel_thingbuf, Receiver as Receiver_thingbuf, Sender as Sender_thingbuf};

static CURRENT_HEARTRATE: AtomicU16 = AtomicU16::new(0);
static LAST_HEART_BEAT_COUNT: AtomicU32 = AtomicU32::new(0);
static STALE_HEART_BEAT_COUNT: AtomicU16 = AtomicU16::new(0);
static NEEDS_REOPEN: AtomicBool = AtomicBool::new(false);

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

pub enum AntStatus {
    NoDevice = 0,
    SetupFailure = 1,
    Connected = 2,
    Disconnected = 3,
}
// outcome of the run method
pub enum Outcome {
    Terminated,
    Failed,
}

pub struct AntHandler {
    router: Router<rusb::Error, UsbDriver<GlobalContext>, TxSender<AntMessage>, RxReceiver<TxMessage>>,
    hr: Display<TxSender<TxMessage>, RxReceiver<AntMessage>>,
    period_ms: Arc<AtomicU32>,
}

impl AntHandler {
    pub fn new(period_ms: Arc<AtomicU32>, status: Arc<AtomicU8>) -> Result<Self, String> {
        // definition for setup failures
        let setup_failure = |err: String| -> String {
            status.store(AntStatus::SetupFailure as u8, Ordering::Relaxed);
            err
        };

        // TODO move this later...
        

        // Try to find USB ANT sticks
        let mut devices: Vec<Device<_>> = DeviceList::new()
            .map_err(|err| setup_failure(format!("USB enumeration: {err}")))?
            .iter()
            .filter(|x| is_ant_usb_device_from_device(x))
            .collect();
        if devices.is_empty() {
            status.store(AntStatus::NoDevice as u8, Ordering::Relaxed);
            return Err("No ANT USB sticks found".into());
            //panic!("No USB ANT adapters found");
        }
        // Just pick the first device we find in the list and set up the driver
        let device = devices.remove(0);
        let driver = UsbDriver::new(device).map_err(|err| setup_failure(format!("driver: {err:?}")))?;
        
        // set up the channels and the router
        let (channel_tx, router_rx) = channel_thingbuf(64);
        let (router_tx, channel_rx) = channel_thingbuf(64);
        let mut router = Router::new( driver, RxReceiver { receiver: router_rx }).map_err(|err| setup_failure(format!("router: {err:?}")))?;
        
        // use the network key to set up the channel and set the timeout
        let snk = SetNetworkKey::new(0, [0xB9, 0xA5, 0x21, 0xFB, 0xBD, 0x72, 0xC3, 0x45]); 
        router.send(&snk).map_err(|err| setup_failure(format!("network key: {err:?}")))?;
        let chan = router.add_channel(TxSender { sender: router_tx }).map_err(|err| setup_failure(format!("ant channel: {err:?}")))?;
        let search_timeout = SearchTimeout::new(chan, 255);
        router.send(&search_timeout).map_err(|err| setup_failure(format!("timeout: {err:?}")))?;
        let config = DisplayConfig { device_number: 0, device_number_extension: 0.into(), channel: chan, period: Period::FourHz, ant_plus_key_index: 0};

        // this is where the heatrate is stored
        let mut hr = Display::new( config, TxSender { sender: channel_tx }, RxReceiver { receiver: channel_rx});

        // save the computed heartrate if we get it 
        hr.set_rx_datapage_callback(Some(|msg| {
            if let Ok(ref page) = msg {
                // get the reported hr and beat count
                let heart_rate = page.common().computed_heart_rate;
                let heart_beat_count = page.common().heart_beat_count;
                // if the current beat count is the same as the last one
                if LAST_HEART_BEAT_COUNT.load(Ordering::Relaxed) == heart_beat_count as u32 {
                    // increment the counter
                    let stale_count = STALE_HEART_BEAT_COUNT.fetch_add(1, Ordering::Relaxed);
                    // if it's been stale for too long, then just report 0
                    if stale_count >= 10 {
                        CURRENT_HEARTRATE.store(0, Ordering::Relaxed);
                    }
                    // otherwise, continue reporting as if everything's ok
                    else {
                        CURRENT_HEARTRATE.store(heart_rate as u16, Ordering::Relaxed);
                    }
                }
                else {
                    STALE_HEART_BEAT_COUNT.store(0, Ordering::Relaxed);
                    CURRENT_HEARTRATE.store(heart_rate as u16, Ordering::Relaxed);
                    LAST_HEART_BEAT_COUNT.store(heart_beat_count as u32, Ordering::Relaxed);
                }
            }
            // println!("{:#?}", msg);
        }));
        // look for timeout messages
        hr.set_rx_message_callback(Some(|msg|{
            if let ant::messages::RxMessage::ChannelEvent(ref event) = msg.message {
                match event.payload.message_code {
                    ant::messages::channel::MessageCode::EventRxFailGoToSearch => {
                        println!("event RX failed, start searching");
                        // set the current heatrate to 0
                        CURRENT_HEARTRATE.store(0, Ordering::Relaxed); 
                    }
                    ant::messages::channel::MessageCode::EventRxSearchTimeout => {
                        println!("event RX timeout failed, indicate need to re open channel");
                        NEEDS_REOPEN.store(true, Ordering::Relaxed);
                    }
                    // fallthrough
                    _ => {}
                }
            }
            // println!("{:#?}", msg);
        }));

        // open and report connected
        hr.open();
        status.store(AntStatus::Connected as u8, Ordering::Relaxed);
        Ok(Self{router, hr, period_ms})
    }

    pub fn run(mut self, thread_ant_term:Arc<AtomicBool>, ctx: egui::Context, heartrate_value_tx: Sender<[f64; 2]>, status: Arc<AtomicU8>, fit_handler: &mut FitHandler, start_fit_time:  f64) -> Outcome {
        let mut last_sent = Instant::now();

        // run this thread in a loop until we get the terminator
        while !thread_ant_term.load(Ordering::Relaxed) {

            match self.router.process() {
                Err(RouterError::DriverError()) => {
                    status.store(AntStatus::Disconnected as u8, Ordering::Relaxed);
                    ctx.request_repaint();
                    eprintln!("ANT USB stick disconnected");
                    return Outcome::Failed
                }
                Err(e) => {
                    println!("Router process error: {:?}", e);
                }
                Ok(_) => {}
            }

            if let Err(e) = self.hr.process() {
                println!("HR process error: {:?}", e);
            }

            if last_sent.elapsed() >= Duration::from_millis(self.period_ms.load(Ordering::Relaxed) as u64) {
                let timestamp = current_fit_time_fine();
                let fit_offset = timestamp - start_fit_time;
                let heart_rate = CURRENT_HEARTRATE.load(Ordering::Relaxed) as f64;
                heartrate_value_tx.send([fit_offset, heart_rate]).map_err(|err| println!("{:?}", err)).ok();
                ctx.request_repaint();
                let _ = fit_handler.update_file(timestamp, heart_rate as u8);
                last_sent = Instant::now();
            }

            if NEEDS_REOPEN.swap(false, Ordering::Relaxed) {
                println!("re-open ANT channel");
                self.hr.open();
            }
            std::thread::sleep(Duration::from_millis(50));

        }
        println!("ANT thread termination received");
        ctx.request_repaint();
        Outcome::Terminated
    }

}
