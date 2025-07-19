use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use tokio::net::TcpListener;
use tokio::time::{interval, sleep};
use thousands::Separable;

static TOTAL_REQUESTS: AtomicU64 = AtomicU64::new(0);
static CONCURRENT_REQUESTS: AtomicU64 = AtomicU64::new(0);

async fn handler(_req: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    CONCURRENT_REQUESTS.fetch_add(1, Ordering::Relaxed);
    sleep(Duration::from_millis(500)).await;
    CONCURRENT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);
    
    let response = r#"{"jsonrpc":"2.0","result":"0xe670ec64341771606e55d6b4ca35a1a6b75ee3d5145a99d05921026d1527331","id":1}"#;
    Ok(Response::new(Full::new(Bytes::from(response))))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 512)]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));
        let mut last_total = 0u64;
        interval.tick().await;
        loop {
            interval.tick().await;
            let current_count = TOTAL_REQUESTS.load(Ordering::Relaxed);
            let rps = current_count - last_total;
            last_total = current_count;
            let concurrent = CONCURRENT_REQUESTS.load(Ordering::Relaxed);
            println!(
                "RPS: {}, Total requests: {}, Concurrent: {}",
                rps.separate_with_commas(),
                current_count.separate_with_commas(),
                concurrent.separate_with_commas()
            );
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 8545));
    let listener = TcpListener::bind(addr).await?;
    println!("Hyper server listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(stream, service_fn(handler))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}