use alloy::primitives::{Address, Bytes};
use alloy::signers::local::PrivateKeySigner;

use crate::tx_gen::queue::PAYLOAD_QUEUE;
use crate::tx_gen::utils::generate_and_sign_tx;

pub fn tx_gen_worker() {
    let mut nonce = 0u64;
    let start_time = std::time::Instant::now();

    let signer = PrivateKeySigner::random();

    loop {
        let tx = generate_and_sign_tx(&signer, 1, nonce, 10_000_000_000, 100_000, Address::from([0; 20]), Bytes::new());
        PAYLOAD_QUEUE.push_payload(tx);
        nonce += 1;

        if nonce == 10_000 {
            let elapsed = start_time.elapsed();
            println!("Time to generate 10k transactions: {:?}", elapsed);
        }
    }
}
