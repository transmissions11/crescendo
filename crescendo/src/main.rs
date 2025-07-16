use std::time::Duration;

const TOTAL_CONNECTIONS: u64 = 4096;
const TARGET_URL: &str = "http://127.0.0.1:8080";

mod stats;
mod utils;
mod worker;

#[tokio::main(worker_threads = 1)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_threads = num_cpus::get() as u64;
    let connections_per_thread = TOTAL_CONNECTIONS / num_threads;

    if let Err(err) = utils::increase_nofile_limit(TOTAL_CONNECTIONS * 10) {
        println!("Failed to increase file descriptor limit: {err}.");
    }

    println!(
        "Running {} threads with {} connections each against {}...",
        num_threads, connections_per_thread, TARGET_URL
    );

    // Start a reporter task with a 1 second measurement/logging interval.
    tokio::spawn(stats::STATS.start_reporter(Duration::from_secs(1)));

    // Spawn all worker threads
    let mut handles = vec![];

    for _ in 0..num_threads {
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let mut tasks = vec![];
                for _ in 0..connections_per_thread {
                    let task = tokio::spawn(worker::connection_worker(TARGET_URL));
                    tasks.push(task);
                }

                // Wait for all tasks (this will run forever since workers loop infinitely)
                for task in tasks {
                    let _ = task.await;
                }
            });
        });
        handles.push(handle);
    }

    // Wait for all threads (this will run forever since workers loop infinitely)
    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}
