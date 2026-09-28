use anyhow::{Result, ensure};
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::packed;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey, ecdsa::Signature};
use sha2::{Digest, Sha256};
use std::str::FromStr;

pub fn digest(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
pub fn bytes(value: &str) -> Result<Vec<u8>> {
    Ok(hex::decode(value.strip_prefix("0x").unwrap_or(value))?)
}
pub fn verify(pubkey: &str, message: &str, signature: &str) -> Result<()> {
    let key = PublicKey::from_slice(&bytes(pubkey)?)?;
    let sig = Signature::from_compact(&bytes(signature)?)?;
    Secp256k1::verification_only().verify_ecdsa(
        &Message::from_digest(Sha256::digest(message.as_bytes()).into()),
        &sig,
        &key,
    )?;
    Ok(())
}
pub fn sign(secret: &SecretKey, message: &str) -> String {
    hex::encode(
        Secp256k1::signing_only()
            .sign_ecdsa(
                &Message::from_digest(Sha256::digest(message.as_bytes()).into()),
                secret,
            )
            .serialize_compact(),
    )
}
pub fn pubkey(secret: &SecretKey) -> String {
    PublicKey::from_secret_key(&Secp256k1::signing_only(), secret).to_string()
}
pub fn address_script(address: &str) -> Result<packed::Script> {
    let address = Address::from_str(address).map_err(anyhow::Error::msg)?;
    ensure!(
        address.network() == NetworkType::Testnet,
        "CKB testnet address required"
    );
    Ok(address.payload().into())
}
pub fn native_address(pubkey: &str) -> Result<String> {
    let key = PublicKey::from_slice(&bytes(pubkey)?)?;
    let payload = AddressPayload::from_pubkey(&key);
    Ok(Address::new(NetworkType::Testnet, payload, true).to_string())
}
pub fn funding_address(script: &serde_json::Value) -> Result<String> {
    let script: ckb_jsonrpc_types::Script = serde_json::from_value(script.clone())?;
    let script: packed::Script = script.into();
    Ok(Address::new(NetworkType::Testnet, AddressPayload::from(script), true).to_string())
}
pub fn same_address(a: &str, b: &str) -> Result<bool> {
    Ok(address_script(a)? == address_script(b)?)
}
pub fn minimum_cell_ckb(address: &str) -> Result<u64> {
    let lock = address_script(address)?;
    let occupied = 8 + 32 + 1 + lock.args().raw_data().len();
    Ok(occupied as u64)
}
pub fn quote_message(quote: &super::model::Quote) -> Result<String> {
    Ok(format!(
        "LiquidLane provider quote\n{}",
        serde_json::to_string(quote)?
    ))
}
