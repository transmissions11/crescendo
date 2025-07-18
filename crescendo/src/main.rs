use std::collections::HashMap;
use std::future::pending;
use std::thread;
use std::time::Duration;

use core_affinity;
use mimalloc::MiMalloc;
use stats::STATS;

use crate::tx_gen::queue::PAYLOAD_QUEUE;
use crate::workers::{assign_workers, WorkerType};

mod network_worker;
mod stats;
mod tx_gen;
mod utils;
mod workers;

#[global_allocator]
// Increases RPS by ~5.5% at the time of
// writing. ~3.3% faster than jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

const TOTAL_CONNECTIONS: u64 = 4096;
const THREAD_PINNING: bool = true;
const TARGET_URL: &str = "http://127.0.0.1:8080";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("Failed to increase file descriptor limit: {err}.");
    }

    let mut core_ids = core_affinity::get_core_ids().unwrap();
    println!("Detected {} effective cores.", core_ids.len());

    // Pin the tokio runtime to a core (if enabled).
    utils::maybe_pin_thread(core_ids.pop().unwrap(), THREAD_PINNING);

    // Given our desired breakdown of workers, translate this into actual numbers of workers to spawn.
    let (workers, worker_counts) = assign_workers(core_ids, vec![(WorkerType::Network, 0.9), (WorkerType::TxGen, 0.1)]);

    let connections_per_network_worker = TOTAL_CONNECTIONS / worker_counts[&WorkerType::Network];
    println!("Connections per network worker: {}", connections_per_network_worker);

    // Spawn the workers, pinning them to the appropriate cores if enabled.
    for (core_id, worker_type) in workers {
        match worker_type {
            WorkerType::TxGen => {
                thread::spawn(move || {
                    utils::maybe_pin_thread(core_id, THREAD_PINNING);
                    tx_gen::worker::tx_gen_worker();
                });
            }
            WorkerType::Network => {
                thread::spawn(move || {
                    utils::maybe_pin_thread(core_id, THREAD_PINNING);
                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

                    rt.block_on(async {
                        for _ in 0..connections_per_network_worker {
                            tokio::spawn(network_worker::network_worker(TARGET_URL));
                        }
                        pending::<()>().await; // Keep the runtime alive forever.
                    });
                });
            }
        }
    }

    // Start reporters.
    tokio::spawn(PAYLOAD_QUEUE.start_reporter(Duration::from_secs(1)));
    tokio::spawn(STATS.start_reporter(Duration::from_secs(1)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
