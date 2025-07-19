use alloy::network::TxSignerSync;
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy_consensus::{SignableTransaction, TxLegacy};

use crate::tx_queue::TX_QUEUE;

const CHAIN_ID: u64 = 1337;

pub fn tx_gen_worker() {
    let mut nonce = 0u64;

    loop {
        let signer = PrivateKeySigner::random();
        let tx = generate_and_sign_tx(&signer, CHAIN_ID, nonce, 0, 100_000, Address::from([0; 20]), Bytes::new());
        TX_QUEUE.push_tx(tx);
        nonce += 1;
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
    let signature = signer.sign_transaction_sync(&mut tx).unwrap();
    let mut payload = Vec::new();
    tx.into_signed(signature).eip2718_encode(&mut payload);
    payload
}
