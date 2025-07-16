use may_minihttp::{HttpServer, HttpService, Request, Response};
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use thousands::Separable;

#[derive(Clone)]
struct HelloWorld {
    request_counter: Arc<AtomicU64>,
}

impl HttpService for HelloWorld {
    fn call(&mut self, _req: Request, res: &mut Response) -> io::Result<()> {
        self.request_counter.fetch_add(1, Ordering::Relaxed);
        res.body("Hello, world!");
        Ok(())
    }
}

fn main() {
    let request_counter = Arc::new(AtomicU64::new(0));

    // Start RPS logging thread
    let counter_clone = Arc::clone(&request_counter);
    thread::spawn(move || {
        let mut last_count = 0u64;
        loop {
            thread::sleep(Duration::from_secs(1));
            let current_count = counter_clone.load(Ordering::Relaxed);
            let rps = current_count - last_count;
            println!(
                "RPS: {}, Total requests: {}",
                rps.separate_with_commas(),
                current_count.separate_with_commas()
            );
            last_count = current_count;
        }
    });

    let server = HttpServer(HelloWorld { request_counter })
        .start("0.0.0.0:8080")
        .unwrap();
    server.join().unwrap();
}
