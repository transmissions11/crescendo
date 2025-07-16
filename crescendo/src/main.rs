use http::StatusCode;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thousands::Separable;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let total_connections = 4096;
    let url = "http://127.0.0.1:8080/";

    println!(
        "Running {} concurrent connections against {}",
        total_connections, url
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
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
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

    // Create a single shared HTTP client with proper connection pooling
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    connector.set_keepalive(Some(Duration::from_secs(60)));

    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(1000) // Allow many connections to the same host
        .http2_only(false) // HTTP/1.1 is often better for load testing
        .retry_canceled_requests(true)
        .set_host(false)
        .build(connector);

    let client = Arc::new(client);

    // Spawn all worker tasks
    let mut tasks = vec![];
    for _ in 0..total_connections {
        let client = Arc::clone(&client);
        let stats = Arc::clone(&stats);
        let task = tokio::spawn(async move {
            worker(url, client, stats).await;
        });
        tasks.push(task);
    }

    // Wait for all tasks (they run forever, so this blocks indefinitely)
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

struct Stats {
    requests: AtomicU64,
    errors: AtomicU64,
}

async fn worker(url: &str, client: Arc<Client<HttpConnector, Empty<Bytes>>>, stats: Arc<Stats>) {
    let req = Request::builder()
        .uri(url)
        .header("Host", "localhost")
        .body(Empty::<Bytes>::new())
        .unwrap();

    loop {
        match client.request(req.clone()).await {
            Ok(res) => {
                // Consume the body to ensure the connection can be reused
                let _ = res.into_body().collect().await;
                stats.requests.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("Request failed: {}", e);
                stats.errors.fetch_add(1, Ordering::Relaxed);
                // Small backoff on error to avoid hammering a failing server
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
