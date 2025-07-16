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
    let connections_per_client = 4; // Each HTTP client handles 16 connections
    let num_clients = total_connections / connections_per_client;
    let url = "http://127.0.0.1:8080/";

    println!(
        "Running {} concurrent connections with {} HTTP clients against {}",
        total_connections, num_clients, url
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

    // Create multiple HTTP clients to reduce contention
    let mut clients = Vec::new();
    for _ in 0..num_clients {
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_keepalive(Some(Duration::from_secs(60)));

        let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(connections_per_client * 2) // Allow some headroom
            .http2_only(false)
            .retry_canceled_requests(true)
            .set_host(false)
            .build(connector);

        clients.push(Arc::new(client));
    }

    // Spawn all worker tasks, distributing them across clients
    let mut tasks = vec![];
    for i in 0..total_connections {
        let client = Arc::clone(&clients[i % num_clients]);
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
