use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use alloy::eips::Decodable2718;
use alloy::primitives::Address;
use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::{Transaction, TxEnvelope};
use axum::body::Bytes;
use axum::routing::get;
use axum::{Json, Router};
use crossbeam_utils::CachePadded;
use dashmap::DashMap;
use mimalloc::MiMalloc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thousands::Separable;
use tokio::time::interval;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

static TOTAL_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));
static CONCURRENT_REQUESTS: CachePadded<AtomicU64> = CachePadded::new(AtomicU64::new(0));

static NONCES: LazyLock<DashMap<Address, u64>> = LazyLock::new(DashMap::new);

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Vec<String>,
    id: u64,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: u64,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

async fn rpc_handler(body: Bytes) -> Json<serde_json::Value> {
    CONCURRENT_REQUESTS.fetch_add(1, Ordering::Relaxed);

    let response = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(value) => {
            // Check if it's a batch request
            if value.is_array() {
                let requests: Vec<JsonRpcRequest> = match serde_json::from_value(value) {
                    Ok(reqs) => reqs,
                    Err(e) => {
                        eprintln!("Failed to parse batch request: {}", e);
                        return Json(json!([{
                            "jsonrpc": "2.0",
                            "error": {"code": -32700, "message": "Parse error"},
                            "id": null
                        }]));
                    }
                };

                let mut responses = Vec::new();
                for req in requests {
                    responses.push(process_single_request(req).await);
                }
                Json(serde_json::Value::Array(
                    responses.into_iter().map(|r| serde_json::to_value(r).unwrap()).collect(),
                ))
            } else {
                // Single request
                match serde_json::from_value::<JsonRpcRequest>(value) {
                    Ok(req) => Json(serde_json::to_value(process_single_request(req).await).unwrap()),
                    Err(e) => {
                        eprintln!("Failed to parse single request: {}", e);
                        Json(json!({
                            "jsonrpc": "2.0",
                            "error": {"code": -32700, "message": "Parse error"},
                            "id": null
                        }))
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse JSON: {}", e);
            Json(json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": "Parse error"},
                "id": null
            }))
        }
    };

    CONCURRENT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    TOTAL_REQUESTS.fetch_add(1, Ordering::Relaxed);

    response
}

async fn process_single_request(req: JsonRpcRequest) -> JsonRpcResponse {
    if req.method == "eth_sendRawTransaction" {
        if let Some(raw_tx) = req.params.first() {
            // Remove "0x" prefix if present
            let tx_hex = raw_tx.strip_prefix("0x").unwrap_or(raw_tx);

            match hex::decode(tx_hex) {
                Ok(tx_bytes) => {
                    // Decode the transaction
                    match TxEnvelope::decode_2718(&mut tx_bytes.as_slice()) {
                        Ok(tx_envelope) => {
                            let sender = tx_envelope.recover_signer().unwrap_or_default();
                            let nonce = tx_envelope.nonce();

                            // Check if nonce is valid
                            let expected_nonce = if let Some(entry) = NONCES.get(&sender) {
                                // Existing sender: should be previous_nonce + 1
                                *entry + 1
                            } else {
                                // New sender: should start at 0
                                0
                            };

                            if nonce != expected_nonce {
                                // Spin for up to 10 seconds waiting for correct nonce
                                let start = Instant::now();
                                let timeout = Duration::from_secs(30);

                                loop {
                                    if start.elapsed() > timeout {
                                        panic!(
                                            "Nonce validation timeout: expected nonce {} but got {} for sender {}",
                                            expected_nonce, nonce, sender
                                        );
                                    }

                                    // Check again if the expected nonce has been updated by another thread
                                    let current_expected = if let Some(current_entry) = NONCES.get(&sender) {
                                        *current_entry + 1
                                    } else {
                                        0
                                    };

                                    if nonce == current_expected {
                                        // Nonce is now valid, break out of loop
                                        println!("Found! {sender} after {} seconds", start.elapsed().as_secs());
                                        break;
                                    }

                                    // Small delay to avoid busy waiting
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                            }

                            // Update nonce tracking
                            NONCES.entry(sender).and_modify(|e| *e = nonce).or_insert(nonce);

                            // Return a fake transaction hash
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                result: Some(format!("0x{:064x}", req.id)),
                                error: None,
                                id: req.id,
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to decode transaction: {}", e);
                            JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32000,
                                    message: format!("Failed to decode transaction: {}", e),
                                }),
                                id: req.id,
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to decode hex: {}", e);
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(JsonRpcError { code: -32000, message: format!("Invalid hex: {}", e) }),
                        id: req.id,
                    }
                }
            }
        } else {
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError { code: -32602, message: "Invalid params".to_string() }),
                id: req.id,
            }
        }
    } else {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError { code: -32601, message: "Method not found".to_string() }),
            id: req.id,
        }
    }
}

#[tokio::main]
async fn main() {
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

                let concurrent = CONCURRENT_REQUESTS.load(Ordering::Relaxed);
                let unique_senders = NONCES.len();
                println!(
                    "RPS: {}, Total requests: {}, Concurrent: {}, Unique senders: {}",
                    rps.separate_with_commas(),
                    current_count.separate_with_commas(),
                    concurrent.separate_with_commas(),
                    unique_senders.separate_with_commas()
                );
            }
        }
    });

    let app = Router::new().route("/", get(rpc_handler).post(rpc_handler));

    println!("Server listening on http://127.0.0.1:8545");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8545").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
