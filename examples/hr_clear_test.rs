use std::sync::atomic::{AtomicU16, AtomicU8, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use ant::channel::{RxError, RxHandler, TxError, TxHandler};
use ant::drivers::{is_ant_usb_device_from_device, UsbDriver};
use ant::messages::config::SetNetworkKey;
use ant::messages::control::ResetSystem;
use ant::plus::profiles::heart_rate::*;
use ant::router::Router;
use dialoguer::Select;
use rusb::{Device, DeviceList};

use thingbuf::mpsc::errors::{TryRecvError, TrySendError};
use thingbuf::mpsc::{channel, Receiver, Sender};

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
            _ => Err(RxError::UnknownError),
        }
    }
}

static LAST_COUNT: AtomicU8 = AtomicU8::new(0);
static LAST_STRAP_TIME: AtomicU16 = AtomicU16::new(0);
static LAST_HOST_TIME: Mutex<Option<Instant>> = Mutex::new(None);

fn on_datapage(result: Result<DataPage, Error>) {
    match result {
        Ok(page) => {
            let common = match &page {
                DataPage::DefaultDataPage(p) => &p.common,
                DataPage::PreviousHeartBeat(p) => &p.common,
                DataPage::CumulativeOperatingTime(p) => &p.common,
                DataPage::ManufacturerInformation(p) => &p.common,
                DataPage::ProductInformation(p) => &p.common,
                DataPage::SwimIntervalSummary(p) => &p.common,
                DataPage::Capabilities(p) => &p.common,
                DataPage::DeviceInformation(p) => &p.common,
                DataPage::BatteryStatus(p) => &p.common,
                DataPage::ManufacturerSpecific(p) => &p.common,
                _ => return,
            };

            let raw_hr = common.computed_heart_rate;
            let count = common.heart_beat_count;
            let strap_time = common.heart_beat_event_time;

            if raw_hr == 0 || raw_hr == 255 {
                return;
            }

            let prev_count = LAST_COUNT.swap(count, Ordering::SeqCst);
            let prev_strap_time = LAST_STRAP_TIME.swap(strap_time, Ordering::SeqCst);

            if count != prev_count {
                let now = Instant::now();

                let count_delta = count.wrapping_sub(prev_count) as f32;
                let strap_time_delta = strap_time.wrapping_sub(prev_strap_time) as f32;

                let strap_calc_bpm = if strap_time_delta > 0.0 {
                    let sec_per_beat = (strap_time_delta / 1024.0) / count_delta;
                    (60.0 / sec_per_beat as f32).round() as u16
                } else {
                    0
                };

                let mut guard = LAST_HOST_TIME.lock().unwrap();
                let host_calc_bpm = if let Some(last_inst) = *guard {
                    let elapsed_sec = now.duration_since(last_inst).as_secs_f32();
                    if elapsed_sec > 0.0 {
                        let sec_per_beat = elapsed_sec / count_delta;
                        (60.0 / sec_per_beat).round() as u16
                    } else {
                        0
                    }
                } else {
                    0
                };
                *guard = Some(now);

                println!(
                    "RAW BYTE: {:3} | STRAP TIMER CALC: {:3} BPM | HOST TIMER CALC: {:3} BPM | ΔBeatCount: {}",
                    raw_hr, strap_calc_bpm, host_calc_bpm, count_delta
                );
            }
        }
        Err(err) => {
            println!("[HR ERROR] DataPage receive error: {:?}", err);
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut devices: Vec<Device<_>> = DeviceList::new()
        .expect("Unable to lookup USB devices")
        .iter()
        .filter(|x| is_ant_usb_device_from_device(x))
        .collect();

    if devices.is_empty() {
        panic!("No ANT devices found");
    }

    let device = if devices.len() == 1 {
        devices.remove(0)
    } else {
        let selection = Select::new()
            .with_prompt("Multiple devices found, please select a radio to use.")
            .items(
                &devices
                    .iter()
                    .map(|x| x.device_descriptor().unwrap())
                    .map(|x| format!("{:04x}:{:04x}", x.vendor_id(), x.product_id()))
                    .collect::<Vec<String>>(),
            )
            .interact()
            .expect("Selection failed");
        devices.remove(selection)
    };

    let driver = UsbDriver::new(device).unwrap();

    let (channel_tx, router_rx) = channel(8);
    let (router_tx, channel_rx) = channel(8);

    let mut router = Router::new(
        driver,
        RxReceiver {
            receiver: router_rx,
        },
    )
    .unwrap();

    let reset_msg = ResetSystem::new();
    router.send(&reset_msg).expect("Failed to reset ANT system");
    sleep(Duration::from_millis(100));
    let _ = router.process();

    let snk = SetNetworkKey::new(0, [0xB9, 0xA5, 0x21, 0xFB, 0xBD, 0x72, 0xC3, 0x45]);
    router.send(&snk).expect("Failed to set network key");

    let chan = router
        .add_channel(TxSender { sender: router_tx })
        .expect("Add channel failed");

    let config = DisplayConfig {
        device_number: 0,
        device_number_extension: 0.into(),
        channel: chan,
        period: Period::FourHz,
        ant_plus_key_index: 0,
    };

    let mut hr = Display::new(
        config,
        TxSender { sender: channel_tx },
        RxReceiver {
            receiver: channel_rx,
        },
    );

    hr.set_rx_datapage_callback(Some(on_datapage));
    hr.open();

    loop {
        router.process().unwrap();
        hr.process().unwrap();
    }
}