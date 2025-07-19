use std::time::Duration;

use alloy::primitives::{hex, Bytes};
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::network_stats::NETWORK_STATS;
use crate::tx_queue::TX_QUEUE;

pub async fn network_worker(url: &str) {
    let mut connector = HttpConnector::new();
    connector.set_nodelay(true);
    connector.set_keepalive(Some(Duration::from_secs(60)));

    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(100)
        .build(connector);

    loop {
        if let Some(tx) = TX_QUEUE.pop_tx() {
            let json_body = format!(
                r#"{{"jsonrpc":"2.0","method":"eth_sendRawTransaction","params":["0x{}"],"id":1}}"#,
                hex::encode(&tx)
            );

            let req = Request::builder()
                .method("POST")
                .uri(url)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(json_body.into_bytes())))
                .unwrap();

            // Spawn the request handling asynchronously to avoid waiting for response
            let client_clone = client.clone();
            tokio::spawn(async move {
                match client_clone.request(req).await {
                    Ok(res) => {
                        if res.status() == StatusCode::OK {
                            // Decode and check the response body
                            match res.into_body().collect().await {
                                Ok(collected) => {
                                    let body_bytes = collected.to_bytes();
                                    let body_str = std::str::from_utf8(&body_bytes).unwrap();

                                    if body_str.contains("\"error\":") {
                                        println!("[!] RPC  response: {}", body_str);
                                        NETWORK_STATS.inc_errors();
                                    } else {
                                        NETWORK_STATS.inc_requests();
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[!] Failed to read response body: {:?}", e);
                                    NETWORK_STATS.inc_errors();
                                }
                            }
                        } else {
                            println!("[!] Request did not have OK status: {:?}", res);
                            NETWORK_STATS.inc_errors();
                        }
                    }
                    Err(e) => {
                        eprintln!("[!] Request failed: {:?}", e);
                        NETWORK_STATS.inc_errors();
                    }
                }
            });
            tokio::time::sleep(Duration::from_millis(100)).await; // Sleep for a bit to avoid overwhelming the server.
        } else {
            // Sleep for a bit while the tx queue repopulates.
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
