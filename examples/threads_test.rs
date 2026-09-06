use std::io::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Error> {
    let thread_1_term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&thread_1_term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&thread_1_term))?;
    let thread_2_term = Arc::clone(&thread_1_term);

    let handle = thread::spawn(move || {
        while !thread_2_term.load(Ordering::Relaxed) {
            println!("thread 1 running");
            thread::sleep(Duration::from_millis(300));
        }
        println!("thread 1 closed");

    });

    while !thread_1_term.load(Ordering::Relaxed) {
        println!("thread 2 running");
        thread::sleep(Duration::from_millis(1000));
    }
    println!("thread 2 closed");
    handle.join().unwrap();
    Ok(())
}