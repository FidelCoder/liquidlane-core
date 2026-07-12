#![allow(dead_code)]

use anyhow::{Result, anyhow};

use crate::domain::{CapacityRequestV2Data, CapacityRequestV2Status, LpReceiptV2Data, VaultV2Data};

const VERSION: u8 = 2;
const VAULT_LEN: usize = 1 + 5 * 8 + 32;
const RECEIPT_LEN: usize = 1 + 6 * 8;
const REQUEST_LEN: usize = 1 + 1 + 3 * 8 + 32 + 32;

pub(super) fn encode_vault_v2(data: &VaultV2Data) -> Result<String> {
    let mut bytes = Vec::with_capacity(VAULT_LEN);
    bytes.push(VERSION);
    push_u64(&mut bytes, data.total);
    push_u64(&mut bytes, data.available);
    push_u64(&mut bytes, data.reserved);
    push_u64(&mut bytes, data.deployed);
    push_u64(&mut bytes, data.fee_balance);
    bytes.extend(hash_bytes(&data.executor_key_hash)?);
    Ok(hex(&bytes))
}

pub(super) fn encode_receipt_v2(data: &LpReceiptV2Data) -> String {
    let mut bytes = Vec::with_capacity(RECEIPT_LEN);
    bytes.push(VERSION);
    push_u64(&mut bytes, data.supplied);
    push_u64(&mut bytes, data.available);
    push_u64(&mut bytes, data.reserved);
    push_u64(&mut bytes, data.deployed);
    push_u64(&mut bytes, data.earned);
    push_u64(&mut bytes, data.claimed);
    hex(&bytes)
}

pub(super) fn encode_request_v2(data: &CapacityRequestV2Data) -> Result<String> {
    let mut bytes = Vec::with_capacity(REQUEST_LEN);
    bytes.push(VERSION);
    bytes.push(status_byte(data.status));
    push_u64(&mut bytes, data.amount);
    push_u64(&mut bytes, data.lease_fee);
    push_u64(&mut bytes, data.expiry);
    bytes.extend(hash_bytes(&data.merchant_lock_hash)?);
    bytes.extend(hash_bytes(&data.fiber_peer_hash)?);
    Ok(hex(&bytes))
}

pub(super) fn decode_vault_v2(hex_data: &str) -> Result<VaultV2Data> {
    let bytes = bytes(hex_data)?;
    if bytes.len() != VAULT_LEN || bytes[0] != VERSION {
        return Err(anyhow!("vault v2 cell data is invalid"));
    }
    Ok(VaultV2Data {
        total: read_u64(&bytes, 1)?,
        available: read_u64(&bytes, 9)?,
        reserved: read_u64(&bytes, 17)?,
        deployed: read_u64(&bytes, 25)?,
        fee_balance: read_u64(&bytes, 33)?,
        executor_key_hash: hex(&bytes[41..73]),
    })
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend(value.to_le_bytes());
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| anyhow!("u64 field missing"))?,
    );
    Ok(u64::from_le_bytes(raw))
}

fn status_byte(status: CapacityRequestV2Status) -> u8 {
    match status {
        CapacityRequestV2Status::Reserved => 1,
        CapacityRequestV2Status::Opening => 2,
        CapacityRequestV2Status::Active => 3,
        CapacityRequestV2Status::Failed => 4,
        CapacityRequestV2Status::Expired => 5,
        CapacityRequestV2Status::Released => 6,
    }
}

fn hash_bytes(value: &str) -> Result<Vec<u8>> {
    let data = bytes(value)?;
    if data.len() == 32 {
        Ok(data)
    } else {
        Err(anyhow!("expected 32-byte hash"))
    }
}

fn bytes(value: &str) -> Result<Vec<u8>> {
    let value = value.trim_start_matches("0x");
    if value.len() % 2 != 0 {
        return Err(anyhow!("hex must have even length"));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| anyhow!("invalid hex data"))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn vault_v2_round_trips() {
        let data = VaultV2Data {
            total: 10,
            available: 8,
            reserved: 2,
            deployed: 0,
            fee_balance: 1,
            executor_key_hash: HASH.to_string(),
        };
        let encoded = encode_vault_v2(&data).unwrap();
        assert_eq!(decode_vault_v2(&encoded).unwrap(), data);
    }

    #[test]
    fn request_v2_has_stable_length() {
        let request = CapacityRequestV2Data {
            merchant_lock_hash: HASH.to_string(),
            amount: 20,
            lease_fee: 1,
            expiry: 99,
            fiber_peer_hash: HASH.to_string(),
            status: CapacityRequestV2Status::Reserved,
        };
        assert_eq!(
            encode_request_v2(&request).unwrap().len(),
            2 + REQUEST_LEN * 2
        );
    }
}
