use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_threads = 14;
    let connections_per_thread = 1024 / num_threads;

    let url = "127.0.0.1:8080";

    println!(
        "Running {} threads with {} connections each against http://{}/",
        num_threads, connections_per_thread, url
    );

    let stats = Arc::new(Stats {
        requests: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });

    let start = Instant::now();

    // Spawn stats thread
    let stats_clone = stats.clone();
    thread::spawn(move || {
        let mut last_requests = 0u64;
        loop {
            thread::sleep(Duration::from_secs(1));
            let requests = stats_clone.requests.load(Ordering::Relaxed);
            let errors = stats_clone.errors.load(Ordering::Relaxed);
            let rps = requests - last_requests;
            last_requests = requests;

            println!("Requests: {} | RPS: {} | Errors: {}", requests, rps, errors);
        }
    });

    // Spawn worker threads
    let mut handles = vec![];
    for _ in 0..num_threads {
        let stats = stats.clone();
        let handle = thread::spawn(move || {
            // Create a runtime for this thread
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut tasks = vec![];
                for _ in 0..connections_per_thread {
                    let stats = stats.clone();
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

    // Final stats
    let total_requests = stats.requests.load(Ordering::Relaxed);
    let total_errors = stats.errors.load(Ordering::Relaxed);
    let elapsed = start.elapsed();
    let avg_rps = total_requests as f64 / elapsed.as_secs_f64();

    println!("\n--- Results ---");
    println!("Total requests: {}", total_requests);
    println!("Total errors: {}", total_errors);
    println!("Duration: {:.2}s", elapsed.as_secs_f64());
    println!("Average RPS: {:.2}", avg_rps);

    Ok(())
}

struct Stats {
    requests: AtomicU64,
    errors: AtomicU64,
}

async fn worker(addr: &str, stats: Arc<Stats>) {
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut buf = vec![0; 512]; // Smaller buffer

    loop {
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                // Disable Nagle's algorithm for lower latency
                let _ = stream.set_nodelay(true);

                // Pipeline multiple requests on same connection
                loop {
                    // Send request
                    if stream.write_all(request).await.is_err() {
                        break;
                    }

                    // Read minimal response
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(_) => {
                            stats.requests.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            stats.errors.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
