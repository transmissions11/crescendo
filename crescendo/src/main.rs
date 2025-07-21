use std::future::pending;
use std::thread;
use std::time::Duration;

use core_affinity;
use mimalloc::MiMalloc;

mod network_stats;
mod tx_queue;
mod utils;
mod workers;

use crate::network_stats::NETWORK_STATS;
use crate::tx_queue::TX_QUEUE;
use crate::workers::{DesireType, WorkerType};

#[global_allocator]
// Increases RPS by ~5.5% at the time of
// writing. ~3.3% faster than jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

// TODO: Configurable CLI args.
const TOTAL_CONNECTIONS: u64 = 50_000; // This is limited by the amount of ephemeral ports available on the system.
const THREAD_PINNING: bool = true;
const TARGET_URL: &str = "http://127.0.0.1:8545";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("[!] Failed to increase file descriptor limit: {err}.");
    }

    let mut core_ids = core_affinity::get_core_ids().unwrap();
    println!("[*] Detected {} effective cores.", core_ids.len());

    // Pin the tokio runtime to a core (if enabled).
    utils::maybe_pin_thread(core_ids.pop().unwrap(), THREAD_PINNING);

    // Given our desired breakdown of workers, translate this into actual numbers of workers to spawn.
    let (workers, worker_counts) = workers::assign_workers(
        core_ids, // Doesn't include the main runtime core.
        vec![(WorkerType::TxGen, DesireType::Percentage(0.3)), (WorkerType::Network, DesireType::Percentage(0.7))],
        THREAD_PINNING, // Only log core ranges if thread pinning is actually enabled.
    );

    let connections_per_network_worker = TOTAL_CONNECTIONS / worker_counts[&WorkerType::Network];
    println!("[*] Connections per network worker: {}", connections_per_network_worker);

    // TODO: Having the assign_workers function do this would be cleaner.
    let mut tx_gen_worker_id = 0;
    let mut network_worker_id = 0;

    println!("[*] Starting workers...");

    // Spawn the workers, pinning them to the appropriate cores if enabled.
    for (core_id, worker_type) in workers {
        match worker_type {
            WorkerType::TxGen => {
                thread::spawn(move || {
                    utils::maybe_pin_thread(core_id, THREAD_PINNING);
                    workers::tx_gen_worker(tx_gen_worker_id);
                });
                tx_gen_worker_id += 1;
            }
            WorkerType::Network => {
                thread::spawn(move || {
                    utils::maybe_pin_thread(core_id, THREAD_PINNING);
                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

                    rt.block_on(async {
                        for i in 0..connections_per_network_worker {
                            tokio::spawn(workers::network_worker(
                                TARGET_URL,
                                (network_worker_id * connections_per_network_worker + i) as usize,
                            ));
                        }
                        pending::<()>().await; // Keep the runtime alive forever.
                    });
                });
                network_worker_id += 1;
            }
        }
    }

    println!("[*] Starting reporters...");

    // Start reporters.
    tokio::spawn(TX_QUEUE.start_reporter(Duration::from_secs(3)));
    tokio::spawn(NETWORK_STATS.start_reporter(Duration::from_secs(3)))
        .await // Keep the main thread alive forever.
        .unwrap();
}
