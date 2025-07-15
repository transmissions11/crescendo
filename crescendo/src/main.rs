use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main(flavor = "multi_thread", worker_threads = 12)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let total_connections = 400;
    let duration_secs = 30;
    let url = "127.0.0.1:8080";
    
    println!("Running {} connections for {}s against http://{}/", 
             total_connections, duration_secs, url);
    
    let stats = Arc::new(Stats {
        requests: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });
    
    let start = Instant::now();
    let end_time = start + Duration::from_secs(duration_secs);
    
    // Spawn stats reporter
    let stats_clone = stats.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut last_requests = 0u64;
        
        loop {
            interval.tick().await;
            let requests = stats_clone.requests.load(Ordering::Relaxed);
            let errors = stats_clone.errors.load(Ordering::Relaxed);
            let rps = requests - last_requests;
            last_requests = requests;
            
            println!("Requests: {} | RPS: {} | Errors: {}", requests, rps, errors);
            
            if Instant::now() >= end_time {
                break;
            }
        }
    });
    
    // Spawn all workers
    let mut handles = vec![];
    for _ in 0..total_connections {
        let stats = stats.clone();
        let handle = tokio::spawn(async move {
            worker(url, stats, end_time).await;
        });
        handles.push(handle);
    }
    
    // Wait for all workers
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
    // Pre-allocate request buffer
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";
    
    while Instant::now() < end_time {
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                // Essential optimizations
                stream.set_nodelay(true).ok();
                
                // Pre-allocate response buffer - only read enough to confirm response
                let mut buf = [0u8; 128];
                
                // Keep connection alive and pipeline requests
                loop {
                    if Instant::now() >= end_time {
                        break;
                    }
                    
                    // Send request
                    if stream.write_all(request).await.is_err() {
                        break;
                    }
                    
                    // Read just enough to confirm we got a response
                    match stream.read(&mut buf).await {
                        Ok(0) => break, // Connection closed
                        Ok(_) => {
                            stats.requests.fetch_add(1, Ordering::Relaxed);
                            // Don't parse response, just continue
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