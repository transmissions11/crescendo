use alloy::network::{TxSigner, TxSignerSync};
use alloy::primitives::{Address, Bytes, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy_consensus::{SignableTransaction, TxLegacy};

pub async fn generate_and_sign_tx(
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

    sign_and_encode_tx(signer, tx).await
}

pub async fn sign_and_encode_tx(signer: &PrivateKeySigner, mut tx: TxLegacy) -> Vec<u8> {
    let signature = signer.sign_transaction(&mut tx).await.unwrap();
    let mut payload = Vec::new();
    tx.into_signed(signature).eip2718_encode(&mut payload);
    payload
}
