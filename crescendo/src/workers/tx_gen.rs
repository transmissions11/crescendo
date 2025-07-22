use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use alloy::network::TxSignerSync;
use alloy::primitives::{Bytes, TxKind, U256};
use alloy::sol;
use alloy::sol_types::SolCall;
use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use rand::Rng;
use rayon::prelude::*;
use thousands::Separable;

use crate::tx_queue::TX_QUEUE;

pub const CHAIN_ID: u64 = 1337;
pub const NUM_ACCOUNTS: u32 = 25_000; // Limited by the number in the genesis (see bin/generate_genesis_alloc.rs)

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

sol! {
    interface ERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

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

        let (signer, recipient) = (
            &SIGNER_LIST[account_index as usize],
            // Send to 1/20th of the accounts, so recipients are pareto-principle distributed.
            SIGNER_LIST[rng.random_range(0..(NUM_ACCOUNTS / 20)) as usize].address(),
        );

        let tx = sign_and_encode_tx(
            signer,
            TxLegacy {
                chain_id: Some(CHAIN_ID),
                nonce,
                gas_price: 100_000_000_000, // 100 gwei
                gas_limit: 100_000,         // 100k gas limit
                to: TxKind::Call(recipient),
                value: U256::ZERO,
                input: ERC20::transferCall { to: recipient, amount: U256::from(rng.random_range(1..=10)) }
                    .abi_encode()
                    .into(),
            },
        );

        TX_QUEUE.push_tx(tx);
    }
}

pub fn sign_and_encode_tx(signer: &PrivateKeySigner, mut tx: TxLegacy) -> Vec<u8> {
    // TODO: Upstream to alloy the ability to use the secp256k1
    // crate instead of k256 for this which is like 5x+ faster.
    let signature = signer.sign_transaction_sync(&mut tx).unwrap();
    let mut payload = Vec::new();
    tx.into_signed(signature).eip2718_encode(&mut payload);
    payload
}
