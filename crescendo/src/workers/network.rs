use std::time::Duration;

use alloy::network::{TxSigner, TxSignerSync};
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy_consensus::{SignableTransaction, TxLegacy};
use http::StatusCode;
use http_body_util::Empty;
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::network_stats::NETWORK_STATS;

pub async fn network_worker(url: &str) {
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    connector.set_keepalive(Some(Duration::from_secs(60)));

    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(100)
        .build(connector);

    let req = Request::builder().uri(url).body(Empty::<Bytes>::new()).unwrap();

    loop {
        match client.request(req.clone()).await {
            Ok(res) => {
                if res.status() == StatusCode::OK {
                    NETWORK_STATS.inc_requests();
                } else {
                    println!("[!] Request did not have OK status: {:?}", res);
                    NETWORK_STATS.inc_errors();
                }
            }
            Err(e) => {
                eprintln!("[!] Request failed: {}", e);
                NETWORK_STATS.inc_errors();
                tokio::time::sleep(Duration::from_millis(10)).await; // Small backoff on error.
            }
        }
    }
}
