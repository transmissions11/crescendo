use http::StatusCode;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Bytes;
use hyper::client::conn;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thousands::Separable;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_threads = 128;
    let connections_per_thread = 4096 / num_threads;

    let url = "http://127.0.0.1:8080/";

    println!(
        "Running {} threads with {} connections each against {}",
        num_threads, connections_per_thread, url
    );

    let stats = Arc::new(Stats {
        requests: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });

    // Start monitoring task
    let stats_clone = Arc::clone(&stats);
    tokio::spawn(async move {
        let mut last_requests = 0u64;
        let mut last_errors = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let requests = stats_clone.requests.load(Ordering::Relaxed);
            let errors = stats_clone.errors.load(Ordering::Relaxed);
            let rps = requests - last_requests;
            let eps = errors - last_errors;
            println!(
                "RPS: {}, EPS: {}, Total requests: {}, Total errors: {}",
                rps.separate_with_commas(),
                eps.separate_with_commas(),
                requests.separate_with_commas(),
                errors.separate_with_commas()
            );
            last_requests = requests;
            last_errors = errors;
        }
    });

    // Spawn all worker tasks
    let mut tasks = vec![];
    let total_connections = num_threads * connections_per_thread;

    for _ in 0..total_connections {
        let stats = Arc::clone(&stats);
        let task = tokio::spawn(async move {
            worker(url, stats).await;
        });
        tasks.push(task);
    }

    // Wait for all tasks (this will run forever since workers loop infinitely)
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

struct Stats {
    requests: AtomicU64,
    errors: AtomicU64,
}

async fn worker(url: &str, stats: Arc<Stats>) {
    // Create HTTP client with connection pooling
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    connector.set_keepalive(Some(Duration::from_secs(60)));

    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(100)
        .retry_canceled_requests(false)
        .build(connector);

    let req = Request::builder()
        .uri(url)
        .body(Empty::<Bytes>::new())
        .unwrap();

    loop {
        match client.request(req.clone()).await {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    stats.requests.fetch_add(1, Ordering::Relaxed);
                } else {
                    println!("Request did not have OK status: {:?}", res);
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => {
                eprintln!("Request failed: {}", e);
                stats.errors.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(10)).await; // Small backoff on error.
            }
        }
    }
}
