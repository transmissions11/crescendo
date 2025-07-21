use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use alloy::network::TxSignerSync;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use rand::Rng;
use rayon::prelude::*;
use thousands::Separable;

use crate::tx_queue::TX_QUEUE;

pub const CHAIN_ID: u64 = 1337;
pub const NUM_ACCOUNTS: u32 = 10000; // Limited by the number in the genesis (see bin/generate_genesis_alloc.rs)

static NONCE_MAP: LazyLock<Mutex<HashMap<u32, u64>>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(NUM_ACCOUNTS as usize);
    for i in 0..NUM_ACCOUNTS {
        map.insert(i, 0);
    }
    Mutex::new(map)
});

static SIGNER_LIST: LazyLock<Vec<PrivateKeySigner>> = LazyLock::new(|| {
    let start = Instant::now();

    let list: Vec<PrivateKeySigner> = (0..NUM_ACCOUNTS)
        .into_par_iter()
        .map(|i| {
            MnemonicBuilder::<English>::default()
                .phrase("test test test test test test test test test test test junk")
                .index(i)
                .unwrap()
                .build()
                .unwrap()
        })
        .collect();
    let duration = start.elapsed();
    println!("[+] Initalized signer list of length {} in {:.1?}", NUM_ACCOUNTS.separate_with_commas(), duration);
    list
});

pub fn tx_gen_worker(_worker_id: u32) {
    let mut rng = rand::rng();

    loop {
        let account_index = rng.random_range(0..NUM_ACCOUNTS);

        // Get and increment nonce atomically.
        let nonce = {
            let mut nonce_map = NONCE_MAP.lock().unwrap();
            let current_nonce = *nonce_map.get(&account_index).unwrap();
            nonce_map.insert(account_index, current_nonce + 1);
            current_nonce
        };

        let signer = &SIGNER_LIST[account_index as usize];
        let recipient = SIGNER_LIST[rng.random_range(0..NUM_ACCOUNTS) as usize].address();

        let tx = generate_and_sign_tx(
            &signer,
            CHAIN_ID,
            nonce,
            100_000_000_000, // 100 gwei
            25_000,          // 25k gas limit
            recipient,
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
        value: U256::from(1),
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
