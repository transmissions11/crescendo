use actix_web::{web, App, HttpResponse, HttpServer, Result};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thousands::Separable;
use tokio::time::interval;

struct Stats {
    total_requests: AtomicU64,
    requests_this_second: AtomicU64,
}

async fn handler(body: web::Bytes, stats: web::Data<Stats>) -> Result<HttpResponse> {
    stats.total_requests.fetch_add(1, Ordering::Relaxed);
    stats.requests_this_second.fetch_add(1, Ordering::Relaxed);

    // Access the request body
    let _ = black_box(String::from_utf8_lossy(&body).to_string());

    Ok(HttpResponse::Ok().body("OK"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let stats = Arc::new(Stats {
        total_requests: AtomicU64::new(0),
        requests_this_second: AtomicU64::new(0),
    });

    // Spawn stats reporter.
    tokio::spawn({
        let stats = stats.clone();
        async move {
            let mut interval = interval(Duration::from_secs(1));
            let mut last_total = 0u64;
            loop {
                interval.tick().await;

                let total = stats.total_requests.load(Ordering::Relaxed);
                let current_second = stats.requests_this_second.swap(0, Ordering::Relaxed);
                let rps = total - last_total;
                last_total = total;

                println!(
                    "Total: {} | RPS: {} | Last Second: {}",
                    total.separate_with_commas(),
                    rps.separate_with_commas(),
                    current_second.separate_with_commas()
                );
            }
        }
    });

    println!("Server listening on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::from(stats.clone()))
            .route("/", web::get().to(handler))
            .route("/", web::post().to(handler))
            .default_service(web::route().to(handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
