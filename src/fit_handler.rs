use std::io::Error;

use std::time::{SystemTime, UNIX_EPOCH};
use embedded_io_adapters::std::FromStd;
use rustyfit::StreamEncoder;
use rustyfit::{Encoder, profile::{mesgdef, typedef}, proto::Message};
use std::{fs::File, io::{BufWriter}};
use rustyfit::profile::typedef::DateTime;

pub fn current_fit_time_fine() -> f64 {
    // Subtract the epoch from the current time. FIT epoch starts 1989-12-31T00:00:00Z which is 631065600
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() - 631065600.0
}

pub struct FitHandler {
    last_ts: u32,
    stream: StreamEncoder<'static, FromStd<BufWriter<File>>>
}

impl FitHandler {
    pub fn create_file () -> Result<Self, Error> {
        //Set up FIT file
        let fout_name = "output.fit";
        let fout = File::create(fout_name)?;
        let bw = BufWriter::new(fout);
        let writer = FromStd::new(bw);
        let encoder: &'static mut Encoder = Box::leak(Box::new(Encoder::new()));
        let mut stream = encoder.stream(writer);

        stream.write_message(&mut {
            let mut file_id = mesgdef::FileId::new();
            file_id.r#type = typedef::File::ACTIVITY;
            file_id.time_created = DateTime(current_fit_time_fine() as u32);
            Message::from(file_id)
        }).map_err(std::io::Error::other)?;

        Ok(Self { last_ts: 0, stream })
    }

    pub fn update_file (&mut self, timestamp: f64, heart_rate: u8) -> Result<(), Error> {
        let current_ts: u32 = timestamp as u32;

        // only save if we are greater than last time
        if current_ts > self.last_ts {
            println!(
                "FIT file updated: HR: {}, current_ts: {}, last_ts: {}",
                heart_rate, current_ts, self.last_ts
            );
            self.stream.write_message(&mut {
                let mut record = mesgdef::Record::new();
                record.timestamp = DateTime(current_ts);
                record.heart_rate = heart_rate;
                Message::from(record)
            }).map_err(std::io::Error::other)?;
            self.last_ts = current_ts;
        }

        Ok(())
    }

    pub fn finish_file (mut self) -> Result<(), Error> {
        self.stream.finish().map_err(std::io::Error::other)?;
        println!("FIT file finished");
        Ok(())
    }
}