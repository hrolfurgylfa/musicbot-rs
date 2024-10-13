use std::thread;
use std::time::Duration;

use parking_lot::deadlock;
use tracing::{debug, error};

pub fn start_deadlock_detection() {
    // Create a background thread which checks for deadlocks every 10s
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let deadlocks = deadlock::check_deadlock();
        debug!("Checking for deadlocks...");
        if deadlocks.is_empty() {
            continue;
        }

        error!("{} deadlocks detected", deadlocks.len());
        for (i, threads) in deadlocks.iter().enumerate() {
            for t in threads {
                let backtrace = t.backtrace();
                error!(
                    ?backtrace,
                    "Deadlock #{}, Thread Id {:#?}",
                    i,
                    t.thread_id()
                );
            }
        }
    });
}
