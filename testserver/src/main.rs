use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use actix_web::{web, App, HttpResponse, HttpServer, Result};
use crossbeam_utils::CachePadded;
use mimalloc::MiMalloc;
use serde_json::json;
use thousands::Separable;
use tokio::time::interval;

#[global_allocator]
// Increases RPS by ~2% at the time of
// writing. About the same as jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

// Depending on build config, false sharing with this static and some
// other frequently accessed memory can occur. To mitigate, we pad it
// to the length of a full cache line to avoid conflict. Minor impact.
static TOTAL_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static CONCURRENT_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));

async fn handler() -> Result<HttpResponse> {
    CONCURRENT_REQUESTS.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(500)).await; // Simulate processing time.
    CONCURRENT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    Ok(HttpResponse::Ok().json(json!({
        "jsonrpc": "2.0",
        "result": "0xe670ec64341771606e55d6b4ca35a1a6b75ee3d5145a99d05921026d1527331", // example tx hash
        "id": 1
    })))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 512)]
async fn main() -> std::io::Result<()> {
    tokio::spawn({
        async move {
            let mut interval = interval(Duration::from_secs(1));
            let mut last_total = 0u64;
            interval.tick().await;
            loop {
                interval.tick().await;

                let current_count = TOTAL_REQUESTS.load(Ordering::Relaxed);
                let rps = current_count - last_total;
                last_total = current_count;

                let concurrent = CONCURRENT_REQUESTS.load(Ordering::Relaxed);
                println!(
                    "RPS: {}, Total requests: {}, Concurrent: {}",
                    rps.separate_with_commas(),
                    current_count.separate_with_commas(),
                    concurrent.separate_with_commas()
                );
            }
        }
    });

    println!("Server listening on http://127.0.0.1:8545");
    println!("Tokio worker threads: 512, Actix workers: 1");

    HttpServer::new(move || App::new().route("/", web::to(handler)))
        .workers(1)  // Force 1 worker to isolate the limit
        .max_connections(5_000_000)
        .max_connection_rate(50_000)
        .backlog(500_000)
        .bind("127.0.0.1:8545")?
        .run()
        .await
}
