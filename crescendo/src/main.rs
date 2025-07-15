use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let num_threads = 12;
    let connections_per_thread = 400 / num_threads;
    let duration_secs = 30;
    let url = "127.0.0.1:8080";

    println!(
        "Running {} threads with {} connections each for {}s against http://{}/",
        num_threads, connections_per_thread, duration_secs, url
    );

    let stats = Arc::new(Stats {
        requests: AtomicU64::new(0),
        errors: AtomicU64::new(0),
    });

    let start = Instant::now();
    let end_time = start + Duration::from_secs(duration_secs);

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

            if Instant::now() >= end_time {
                break;
            }
        }
    });

    // Spawn worker threads
    let mut handles = vec![];
    for _ in 0..num_threads {
        let stats = stats.clone();
        let handle = thread::spawn(move || {
            worker_thread(url, connections_per_thread, stats, end_time);
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

fn worker_thread(addr: &str, num_connections: usize, stats: Arc<Stats>, end_time: Instant) {
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    
    // Run connections sequentially in this thread
    for _ in 0..num_connections {
        if Instant::now() >= end_time {
            break;
        }
        
        // Keep reconnecting and sending requests
        while Instant::now() < end_time {
            match TcpStream::connect(addr) {
                Ok(mut stream) => {
                    stream.set_nodelay(true).ok();
                    let mut buf = [0u8; 256];
                    
                    // Send requests on this connection until it fails or time is up
                    loop {
                        if Instant::now() >= end_time {
                            break;
                        }
                        
                        // Send request
                        if stream.write_all(request).is_err() {
                            break;
                        }
                        
                        // Read response header only
                        match stream.read(&mut buf) {
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
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}