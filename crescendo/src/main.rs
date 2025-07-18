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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("Failed to increase file descriptor limit: {err}.");
    }

    let mut core_ids = core_affinity::get_core_ids().unwrap();
    let mut total_cores = core_ids.len() as u64;

    let connections_per_thread = TOTAL_CONNECTIONS / total_cores as u64;
    println!(
        "Running {} threads with {} connections each against {}...",
        total_cores, connections_per_thread, TARGET_URL
    );

    core_ids.reverse(); // So we're popping id 0 first.

    utils::pin_thread(core_ids.pop().unwrap()); // Pin the tokio runtime to a core.

    while let Some(core_id) = core_ids.pop() {
        let cores_left = core_ids.len() as u64;
        if cores_left < total_cores * 1 / 10 {
            println!("Spawning tx gen worker on core {}", core_id.id);
            thread::spawn(move || {
                utils::pin_thread(core_id);
                tx_gen::worker::tx_gen_worker();
            });
        } else {
            println!("Spawning connection worker on core {}", core_id.id);
            thread::spawn(move || {
                utils::pin_thread(core_id);
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    for _ in 0..connections_per_thread {
                        tokio::spawn(worker::connection_worker(TARGET_URL));
                    }
                    pending::<()>().await; // Keep the runtime alive forever.
                });
            });
        }
    }

    // Start reporters.
    tokio::spawn(PAYLOAD_QUEUE.start_reporter(Duration::from_secs(1)));
    tokio::spawn(STATS.start_reporter(Duration::from_secs(1)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
