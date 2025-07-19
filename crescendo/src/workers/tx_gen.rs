use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};

use alloy::network::TxSignerSync;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};

use crate::tx_queue::TX_QUEUE;

const CHAIN_ID: u64 = 1337;

pub fn tx_gen_worker(worker_id: u32) {
    let mut nonce = 0u64;

    let signer = MnemonicBuilder::<English>::default()
        .phrase("test test test test test test test test test test test junk")
        .index(worker_id)
        .unwrap()
        .build()
        .unwrap();

    loop {
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
        nonce += 1;
        if nonce % 10000 == 0 {
            println!("[*] TxGen worker {} submitted {} txs.", worker_id, nonce);
        }
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
