use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Address, Bytes};

use crate::tx_gen::utils::generate_and_sign_tx;

pub fn tx_gen_worker(thread_id: usize) {
    let tx_counter = Arc::new(AtomicU64::new(0));

    {
        let tx_counter = tx_counter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_nonce = 0;
            interval.tick().await;
            loop {
                interval.tick().await;
                let current_nonce = tx_counter.load(Ordering::Relaxed);
                let tps = current_nonce - last_nonce;
                println!("Thread {} TXs per second: {}", thread_id, tps);
                last_nonce = current_nonce;
            }
        });
    }

    let mut nonce = 0u64;

    loop {
        let tx = generate_and_sign_tx(1, nonce, 10_000_000_000, 100_000, Address::from([0; 20]), Bytes::new());
        black_box(tx);
        nonce += 1;
        tx_counter.fetch_add(1, Ordering::Relaxed);
    }
}
