use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_threads = 128;
    let connections_per_thread = 4096 / num_threads;

    let url = "127.0.0.1:8080";

    println!(
        "Running {} threads with {} connections each against http://{}/",
        num_threads, connections_per_thread, url
    );

    // Spawn worker threads
    let mut handles = vec![];
    for _ in 0..num_threads {
        let handle = thread::spawn(move || {
            // Create a runtime for this thread
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut tasks = vec![];
                for _ in 0..connections_per_thread {
                    let task = tokio::spawn(async move {
                        worker(url).await;
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

async fn worker(addr: &str) {
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";

    loop {
        println!("Connecting to {}", addr);
        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                // Disable Nagle's algorithm for lower latency
                let _ = stream.set_nodelay(true);

                // Pipeline multiple requests on same connection
                loop {
                    // Send request
                    if let Err(e) = stream.write_all(request).await {
                        println!("Write error: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                println!("TCP Connection Error: {}", e);
            }
        }
    }
}
