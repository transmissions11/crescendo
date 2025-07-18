use std::future::pending;
use std::thread;
use std::time::Duration;

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
    let num_threads = num_cpus::get() as u64;
    let connections_per_thread = TOTAL_CONNECTIONS / num_threads;

    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("Failed to increase file descriptor limit: {err}.");
    }

    println!(
        "Running {} threads with {} connections each against {}...",
        num_threads, connections_per_thread, TARGET_URL
    );

    // Spawn all worker threads.
    for _ in 0..(num_threads / 2) {
        thread::spawn(move || tx_gen::worker::tx_gen_worker());

        // thread::spawn(move || {
        //     let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        //     rt.block_on(async {
        //         for _ in 0..connections_per_thread {
        //             tokio::spawn(worker::connection_worker(TARGET_URL));
        //         }
        //         pending::<()>().await; // Keep the runtime alive forever.
        //     });
        // });
    }

    // Start reporters.
    tokio::spawn(PAYLOAD_QUEUE.start_reporter(Duration::from_secs(1)));
    tokio::spawn(STATS.start_reporter(Duration::from_secs(1)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
