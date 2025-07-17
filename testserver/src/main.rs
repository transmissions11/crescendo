use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use actix_web::{web, App, HttpResponse, HttpServer, Result};
use crossbeam_utils::CachePadded;
use mimalloc::MiMalloc;
use thousands::Separable;
use tokio::time::interval;

#[global_allocator]
// Increases RPS by ~2% at the time of
// writing. About the same as jemalloc.
static GLOBAL: MiMalloc = MiMalloc;

// Depending on build config, false sharing with this static and some
// other frequently accessed memory can occur. To mitigate, we pad it
// to the length of a full cache line to avoid conflict. Minor impact.
static TOTAL_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));

async fn handler() -> Result<HttpResponse> {
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tokio::spawn({
        async move {
            let mut interval = interval(Duration::from_secs(1));
            let mut last_total = 0u64;
            interval.tick().await;
            loop {
                interval.tick().await;

                let current_count = TOTAL_REQUESTS.load(Ordering::Relaxed);
                let rps = current_count - last_total;
                last_total = current_count;

                println!(
                    "RPS: {}, Total requests: {}",
                    rps.separate_with_commas(),
                    current_count.separate_with_commas()
                );
            }
        }
    });

    println!("Server listening on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .route("/", web::get().to(handler))
            .route("/", web::post().to(handler))
            .default_service(web::route().to(handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
