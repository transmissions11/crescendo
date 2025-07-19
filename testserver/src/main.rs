use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::routing::post;
use axum::{Json, Router};
use crossbeam_utils::CachePadded;
use mimalloc::MiMalloc;
use serde_json::json;
use thousands::Separable;
use tokio::time::interval;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

static TOTAL_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static CONCURRENT_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));

async fn handler() -> Json<serde_json::Value> {
    CONCURRENT_REQUESTS.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(500)).await; // Simulate processing time.
    CONCURRENT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    Json(json!({
        "jsonrpc": "2.0",
        "result": "hello world!",
        "id": 1
    }))
}

#[tokio::main]
async fn main() {
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

    // build our application with a route
    let app = Router::new().route("/", post(handler));

    println!("Server listening on http://127.0.0.1:8545");

    // run our app with axum
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8545").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
