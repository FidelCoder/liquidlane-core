const CKB_PREFIXES: [&str; 2] = ["ckb1", "ckt1"];
const BECH32_CHARS: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

pub fn is_plausible_ckb_address(address: &str) -> bool {
    let address = address.trim();
    if address.len() < 42 || address.len() > 512 {
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

    #[test]
    fn accepts_long_script_address() {
        assert!(is_plausible_ckb_address(
            "ckt1qz898g3qcc6x78td8yxq3304flmn5eqwjs8cs63qcmwuyesc5ap4wqjkg67zcsv3rnhkmqqpfp7p8x7hglmkq6hfm56s4gma8zvlfp9xdn5cxdrxqtjxx286nkllj32q5d7tfnxxxss2lywljzy4v4fac86vxcfkwkda2uqpgqw3jd56fhjjf7dpnwyrn6t6pl4pql3s3l5ck7ef9mxjasv6p0jhqldlv8xch46cmprnt04s2g9xftwanln239mjyfkm83tu8mzpsql2zff9pl79rucgw0nql2p8aa2m8q4scdhwnlx5ysqt5j8y6"
        ));
    }
}
