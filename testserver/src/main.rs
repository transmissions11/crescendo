use actix_web::{web, App, HttpResponse, HttpServer, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

struct Stats {
    total_requests: AtomicU64,
    requests_this_second: AtomicU64,
    long_body: AtomicBool,
}

async fn handler(stats: web::Data<Stats>) -> Result<HttpResponse> {
    stats.total_requests.fetch_add(1, Ordering::Relaxed);
    stats.requests_this_second.fetch_add(1, Ordering::Relaxed);
    if stats.long_body.load(Ordering::Relaxed) {
        Ok(HttpResponse::Ok().body(
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nulla non ex tellus. Proin consectetur urna pretium interdum tempor. Etiam volutpat elit in felis lobortis, luctus pellentesque tellus efficitur. In orci neque, pellentesque vel magna sit amet, viverra lobortis nunc. Quisque quis massa quis dui dictum placerat. Donec bibendum ut augue ut posuere. Nunc quis nibh massa. Etiam aliquam sem ut enim rhoncus, at semper nibh vestibulum. Etiam rhoncus accumsan odio, a varius urna aliquet non. Mauris nulla risus, pretium eu vestibulum sit amet, imperdiet eu lacus. Suspendisse faucibus lectus ut nisl bibendum, at gravida risus tempus. Donec pharetra nisi eu lectus egestas, quis porttitor libero semper. Phasellus eu velit mi. Integer sit amet ullamcorper odio, ac feugiat sem. Cras hendrerit tortor a metus venenatis, a iaculis lorem volutpat.",
        ))
    } else {
        Ok(HttpResponse::Ok().body("OK"))
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let initial_long_body = args.contains(&"--long-body".to_string());

    let stats = Arc::new(Stats {
        total_requests: AtomicU64::new(0),
        requests_this_second: AtomicU64::new(0),
        long_body: AtomicBool::new(initial_long_body),
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
                    format_with_commas(total),
                    format_with_commas(rps),
                    format_with_commas(current_second)
                );
            }
        }
    });

    // Spawn long body toggler.
    tokio::spawn({
        let stats = stats.clone();
        async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                let current = stats.long_body.load(Ordering::Relaxed);
                let new_value = !current;
                stats.long_body.store(new_value, Ordering::Relaxed);
                println!("Long body switched to: {}", new_value);
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

fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut chars: Vec<char> = s.chars().collect();
    chars.reverse();

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }

    result.chars().rev().collect()
}
