use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::interval;

// Global counters using atomics for lock-free performance
struct Stats {
    total_requests: AtomicU64,
    requests_this_second: AtomicU64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use all available CPU cores
    println!("Starting server with {} threads", num_cpus::get());

    let stats = Arc::new(Stats {
        total_requests: AtomicU64::new(0),
        requests_this_second: AtomicU64::new(0),
    });

    // Spawn stats reporter
    let stats_clone = stats.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));
        let mut last_total = 0u64;

        loop {
            interval.tick().await;

            let total = stats_clone.total_requests.load(Ordering::Relaxed);
            let current_second = stats_clone.requests_this_second.swap(0, Ordering::Relaxed);
            let rps = total - last_total;
            last_total = total;

            println!(
                "Total: {} | RPS: {} | Last Second: {}",
                total, rps, current_second
            );
        }
    });

    // Bind to TCP socket with SO_REUSEPORT for better load distribution
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Server listening on http://127.0.0.1:8080");

    // Pre-create response as static bytes for zero allocation
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nOK";

    loop {
        let (mut socket, _) = listener.accept().await?;
        let stats = stats.clone();

        // Spawn task for each connection
        tokio::spawn(async move {
            // Disable Nagle's algorithm for lower latency
            let _ = socket.set_nodelay(true);

            // Use smaller buffer to reduce allocations
            let mut buf = [0u8; 512];

            loop {
                // Read request (minimal parsing for speed)
                match socket.read(&mut buf).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => {
                        // Quick check for HTTP request end (double CRLF)
                        if n >= 4 && buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                            // Increment counters with relaxed ordering for speed
                            stats.total_requests.fetch_add(1, Ordering::Relaxed);
                            stats.requests_this_second.fetch_add(1, Ordering::Relaxed);

                            // Write response directly without buffering
                            if socket.write_all(response).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}
