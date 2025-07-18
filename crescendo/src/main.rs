use std::future::pending;
use std::thread;
use std::time::Duration;

use core_affinity;
use mimalloc::MiMalloc;
use stats::STATS;

use crate::tx_gen::queue::PAYLOAD_QUEUE;

mod stats;
mod tx_gen;
mod utils;
mod worker;

#[global_allocator]
// Increases RPS by ~5.5% at the time of
// writing. ~3.3% faster than jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

const TOTAL_CONNECTIONS: u64 = 4096;
const TARGET_URL: &str = "http://127.0.0.1:8080";

#[tokio::main(worker_threads = 1)]
async fn main() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let mut threads_available = core_ids.len() as u64;
    let connections_per_thread = TOTAL_CONNECTIONS / threads_available as u64;

    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("Failed to increase file descriptor limit: {err}.");
    }

    println!(
        "Running {} threads with {} connections each against {}...",
        threads_available, connections_per_thread, TARGET_URL
    );

    // TODO: Make this ratio configurable, and also maybe auto-tune/warn if its too low?
    for i in 0..(threads_available * 3 / 10) {
        let to_pin = core_ids[i as usize];
        thread::spawn(move || {
            core_affinity::set_for_current(to_pin);
            tx_gen::worker::tx_gen_worker();
        });
        threads_available -= 1;
    }

    println!("how many left: {:?}", core_affinity::get_core_ids().unwrap().len());

    for _ in 0..threads_available {
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                for _ in 0..connections_per_thread {
                    tokio::spawn(worker::connection_worker(TARGET_URL));
                }
                pending::<()>().await; // Keep the runtime alive forever.
            });
        });
    }

    // Start reporters.
    tokio::spawn(PAYLOAD_QUEUE.start_reporter(Duration::from_secs(1)));
    tokio::spawn(STATS.start_reporter(Duration::from_secs(1)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
