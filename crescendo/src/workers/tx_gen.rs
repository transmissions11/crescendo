use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use alloy::network::TxSignerSync;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use rand::Rng;

use crate::tx_queue::TX_QUEUE;

const CHAIN_ID: u64 = 1337;
const NUM_ACCOUNTS: u32 = 20;

// Static hashmap to track nonces for all accounts (0 to 9999)
static NONCE_MAP: LazyLock<Arc<Mutex<HashMap<u32, u64>>>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(NUM_ACCOUNTS as usize);
    // Initialize all accounts with nonce 0.
    for i in 0..NUM_ACCOUNTS {
        map.insert(i, 0);
    }
    Arc::new(Mutex::new(map))
});

pub fn tx_gen_worker(_worker_id: u32) {
    let mut rng = rand::rng();

    loop {
        // Randomly select an account index
        let account_index = rng.random_range(0..NUM_ACCOUNTS);

        // Get and increment nonce atomically
        let nonce = {
            let mut nonce_map = NONCE_MAP.lock().unwrap();
            let current_nonce = *nonce_map.get(&account_index).unwrap();
            nonce_map.insert(account_index, current_nonce + 1);
            current_nonce
        };

        // Create signer for this account index
        let signer = MnemonicBuilder::<English>::default()
            .phrase("test test test test test test test test test test test junk")
            .index(account_index)
            .unwrap()
            .build()
            .unwrap();

        let tx = generate_and_sign_tx(
            &signer,
            CHAIN_ID,
            nonce,
            100_000_000_000, // 100 gwei
            25_000,          // 25k gas limit
            Address::from([0; 20]),
            Bytes::new(),
        );
        TX_QUEUE.push_tx(tx);
    }
}

pub fn generate_and_sign_tx(
    signer: &PrivateKeySigner,
    chain_id: u64,
    nonce: u64,
    gas_price: u128,
    gas_limit: u64,
    to: Address,
    data: Bytes,
) -> Vec<u8> {
    let tx = TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price,
        gas_limit,
        to: TxKind::Call(to),
        value: U256::ZERO,
        input: data,
    };

    sign_and_encode_tx(signer, tx)
}

pub fn sign_and_encode_tx(signer: &PrivateKeySigner, mut tx: TxLegacy) -> Vec<u8> {
    // TODO: Upstream to alloy the ability to use the secp256k1
    // crate instead of k256 for this which is like 5x+ faster.
    let signature = signer.sign_transaction_sync(&mut tx).unwrap();
    let mut payload = Vec::new();
    tx.into_signed(signature).eip2718_encode(&mut payload);
    payload
}
