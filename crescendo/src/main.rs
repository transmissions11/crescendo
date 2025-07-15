use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Start monitoring thread
    let stats_clone = Arc::clone(&stats);
    thread::spawn(move || {
        let mut last_requests = 0u64;
        let mut last_errors = 0u64;
        loop {
            thread::sleep(Duration::from_secs(1));
            let requests = stats_clone.requests.load(Ordering::Relaxed);
            let errors = stats_clone.errors.load(Ordering::Relaxed);
            let rps = requests - last_requests;
            let eps = errors - last_errors;
            println!(
                "RPS: {}, EPS: {}, Total requests: {}, Total errors: {}",
                rps, eps, requests, errors
            );
            last_requests = requests;
            last_errors = errors;
        }
    });

    // Spawn worker threads
    let mut handles = vec![];
    for _ in 0..num_threads {
        let stats = Arc::clone(&stats);
        let handle = thread::spawn(move || {
            // Create a runtime for this thread
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut tasks = vec![];
                for _ in 0..connections_per_thread {
                    let stats = Arc::clone(&stats);
                    let task = tokio::spawn(async move {
                        worker(url, stats).await;
                    });
                    tasks.push(task);
                }
                // Wait for all tasks
                for task in tasks {
                    let _ = task.await;
                }
            });
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
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
        .retry_canceled_requests(true)
        .set_host(false)
        .build(connector);

    loop {
        // Create request
        let req = match Request::builder()
            .uri(url)
            .header("Host", "localhost")
            .body(Empty::<Bytes>::new())
        {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to build request: {}", e);
                stats.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        // Send request
        match client.request(req).await {
            Ok(res) => {
                stats.requests.fetch_add(1, Ordering::Relaxed);
                // Consume the body to complete the request
                let body = res.into_body();
                let body_bytes = body.collect().await;
                println!("Request completed: {:?}", body_bytes);
            }
            Err(e) => {
                eprintln!("Request failed: {}", e);
                stats.errors.fetch_add(1, Ordering::Relaxed);
                // Small backoff on error
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}
