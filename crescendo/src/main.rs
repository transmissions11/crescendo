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

    let mut spawned_threads: u64 = 0;
    for core_id in core_ids {
        if spawned_threads < threads_available * 3 / 10 {
            println!("Spawning tx gen worker on core {}", core_id.id);
            thread::spawn(move || {
                // core_affinity::set_for_current(core_id);
                core_affinity::set_for_current(core_id);
                tx_gen::worker::tx_gen_worker();
            });
        } else {
            println!("Spawning connection worker on core {}", core_id.id);
            thread::spawn(move || {
                core_affinity::set_for_current(core_id);
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    for _ in 0..connections_per_thread {
                        tokio::spawn(worker::connection_worker(TARGET_URL));
                    }
                    pending::<()>().await; // Keep the runtime alive forever.
                });
            });
        }
        spawned_threads += 1;
    }

    // Start reporters.
    tokio::spawn(PAYLOAD_QUEUE.start_reporter(Duration::from_secs(1)));
    tokio::spawn(STATS.start_reporter(Duration::from_secs(1)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
