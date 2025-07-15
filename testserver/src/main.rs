use actix_web::{web, App, HttpResponse, HttpServer, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

// Global counters using atomics for lock-free performance
struct Stats {
    total_requests: AtomicU64,
    requests_this_second: AtomicU64,
}

async fn handler(stats: web::Data<Stats>) -> Result<HttpResponse> {
    // Increment counters
    stats.total_requests.fetch_add(1, Ordering::Relaxed);
    stats.requests_this_second.fetch_add(1, Ordering::Relaxed);

    Ok(HttpResponse::Ok().body("OK"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
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

    println!("Server listening on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::from(stats.clone()))
            .route("/", web::get().to(handler))
            .route("/", web::post().to(handler))
            .route("/", web::put().to(handler))
            .route("/", web::delete().to(handler))
            .route("/", web::head().to(handler))
            .route("/", web::patch().to(handler))
            .default_service(web::route().to(handler))
    })
    .workers(num_cpus::get())
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
