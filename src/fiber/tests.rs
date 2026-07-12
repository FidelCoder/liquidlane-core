use super::funding_amount_hex;

#[test]
fn converts_ckb_funding_amount_to_hex_shannons() {
    assert_eq!(funding_amount_hex("CKB", 499).unwrap(), "0xb9e459300");
}

#[test]
fn leaves_udt_funding_amount_in_asset_units() {
    assert_eq!(funding_amount_hex("RUSD", 20_000_000).unwrap(), "0x1312d00");
}
