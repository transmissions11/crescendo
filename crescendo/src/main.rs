use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let threads = 12;
    let connections_per_thread = 400 / threads;
    let duration_secs = 30;
    let url = "127.0.0.1:8080";
    
    println!("Running {} threads with {} connections each for {}s against http://{}/", 
             threads, connections_per_thread, duration_secs, url);
    
    let stats = Arc::new(Stats {
        requests: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });
    
    // Start time
    let start = Instant::now();
    let end_time = start + Duration::from_secs(duration_secs);
    
    // Spawn stats reporter
    let stats_clone = stats.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));
        let mut last_requests = 0u64;
        
        loop {
            interval.tick().await;
            let requests = stats_clone.requests.load(Ordering::Relaxed);
            let errors = stats_clone.errors.load(Ordering::Relaxed);
            let rps = requests - last_requests;
            last_requests = requests;
            
            println!("Requests: {} | RPS: {} | Errors: {}", requests, rps, errors);
        }
    });
    
    // Spawn threads
    let mut handles = vec![];
    for _ in 0..threads {
        let stats = stats.clone();
        let handle = tokio::spawn(async move {
            // Each thread manages multiple connections
            let mut connection_handles = vec![];
            for _ in 0..connections_per_thread {
                let stats = stats.clone();
                let h = tokio::spawn(async move {
                    worker(url, stats, end_time).await;
                });
                connection_handles.push(h);
            }
            // Wait for all connections in this thread
            for h in connection_handles {
                let _ = h.await;
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        let _ = handle.await;
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

async fn worker(addr: &str, stats: Arc<Stats>, end_time: Instant) {
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";
    let mut buf = vec![0; 4096];
    
    // Reuse connection
    loop {
        if Instant::now() >= end_time {
            break;
        }
        
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                // Keep using this connection until it fails
                loop {
                    if Instant::now() >= end_time {
                        break;
                    }
                    
                    // Send request
                    if stream.write_all(request).await.is_err() {
                        break;
                    }
                    
                    // Read response (minimal parsing)
                    match stream.read(&mut buf).await {
                        Ok(0) => break, // Connection closed
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
                // Brief pause before retry
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }
}