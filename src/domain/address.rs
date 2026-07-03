const CKB_PREFIXES: [&str; 2] = ["ckb1", "ckt1"];
const BECH32_CHARS: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

pub fn is_plausible_ckb_address(address: &str) -> bool {
    let address = address.trim();
    if address.len() < 42 || address.len() > 128 {
        return false;
    }
    if address.chars().any(|ch| ch.is_ascii_uppercase()) {
        return false;
    }
    if address.contains("liquidlane") || address.contains("vault") {
        return false;
    }

    let Some(payload) = CKB_PREFIXES
        .iter()
        .find_map(|prefix| address.strip_prefix(prefix))
    else {
        return false;
    };

    !payload.is_empty() && payload.chars().all(|ch| BECH32_CHARS.contains(ch))
}

#[cfg(test)]
mod tests {
    use super::is_plausible_ckb_address;

    #[test]
    fn rejects_placeholder_vault_address() {
        assert!(!is_plausible_ckb_address(
            "ckt1qpkp7liquidlanevault000000000000000000000000000"
        ));
    }

    #[test]
    fn accepts_fixture_with_ckb_prefix_and_bech32_charset() {
        assert!(is_plausible_ckb_address(
            "ckt1qpkp7qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"
        ));
    }
}
