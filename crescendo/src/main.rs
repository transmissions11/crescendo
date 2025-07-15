use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connections = 400;
    let duration_secs = 30;
    let url = "127.0.0.1:8080";
    
    println!("Running {} connections for {}s against http://{}/", connections, duration_secs, url);
    
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
    
    // Spawn workers
    let mut handles = vec![];
    for _ in 0..connections {
        let stats = stats.clone();
        let handle = tokio::spawn(async move {
            worker(url, stats, end_time).await;
        });
        handles.push(handle);
    }
    
    // Wait for duration
    tokio::time::sleep(Duration::from_secs(duration_secs)).await;
    
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
    let request = b"GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let mut buf = vec![0; 1024];
    
    while Instant::now() < end_time {
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                // Send request
                if stream.write_all(request).await.is_ok() {
                    // Read response
                    match stream.read(&mut buf).await {
                        Ok(_) => {
                            stats.requests.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            stats.errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                } else {
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}