use serde_json::Value;

#[derive(Clone, Debug)]
pub struct FiberChannel {
    pub channel_id: Option<String>,
    pub temporary_channel_id: Option<String>,
    pub peer_pubkey: Option<String>,
    pub amount_ckb: Option<u64>,
    pub funding_tx_hash: Option<String>,
    pub funding_out_point: Option<String>,
    pub settlement_tx_hash: Option<String>,
    pub is_usable: bool,
    pub is_closed: bool,
    pub is_failed: bool,
}

pub(super) fn channel_from_value(value: &Value) -> FiberChannel {
    let channel_id = string_field_any(value, &["channel_id", "id"]);
    let temporary_channel_id =
        string_field_any(value, &["temporary_channel_id", "temp_channel_id"]);
    let peer_pubkey = string_field_any(
        value,
        &["peer_pubkey", "remote_pubkey", "remote_node_id", "pubkey"],
    );
    let state = string_field_any(
        value,
        &[
            "state",
            "status",
            "channel_state",
            "state_name",
            "state_flags",
        ],
    );
    let amount_ckb =
        string_field_any(value, &["funding_amount", "local_balance"]).and_then(hex_shannons_to_ckb);
    let funding_tx_hash = string_field_any(value, &["funding_tx_hash", "funding_txid", "tx_hash"]);
    let funding_out_point = string_field_any(
        value,
        &["funding_out_point", "funding_outpoint", "out_point"],
    );
    let settlement_tx_hash = string_field_any(
        value,
        &["settlement_tx_hash", "closing_tx_hash", "close_tx_hash"],
    );
    let failure_state =
        string_field_any(value, &["state_flags", "failure_reason", "error", "reason"]);
    let is_failed = channel_failed(state.as_deref()) || channel_failed(failure_state.as_deref());
    let is_closed = channel_closed(state.as_deref()) || settlement_tx_hash.is_some();
    let is_usable = channel_usable(channel_id.as_deref(), state.as_deref());
    FiberChannel {
        channel_id,
        temporary_channel_id,
        peer_pubkey,
        amount_ckb,
        funding_tx_hash,
        funding_out_point,
        settlement_tx_hash,
        is_usable,
        is_closed,
        is_failed,
    }
}

fn channel_usable(channel_id: Option<&str>, state: Option<&str>) -> bool {
    let Some(channel_id) = channel_id.filter(|value| !value.trim().is_empty()) else {
        return false;
    };
    let Some(state) = state else {
        return !channel_id.is_empty();
    };
    let state = state.to_ascii_lowercase();
    if channel_failed(Some(&state)) {
        return false;
    }
    state.contains("ready")
        || state.contains("open")
        || state.contains("active")
        || state.contains("normal")
}

fn channel_closed(state: Option<&str>) -> bool {
    let Some(state) = state else {
        return false;
    };
    let state = state.to_ascii_lowercase();
    ["closed", "shutdown", "settled"]
        .iter()
        .any(|needle| state.contains(needle))
}

fn channel_failed(state: Option<&str>) -> bool {
    let Some(state) = state else {
        return false;
    };
    let state = state.to_ascii_lowercase();
    ["failed", "abandoned", "aborted"]
        .iter()
        .any(|needle| state.contains(needle))
}

fn hex_shannons_to_ckb(value: String) -> Option<u64> {
    let raw = value.strip_prefix("0x").unwrap_or(value.as_str());
    u128::from_str_radix(raw, 16)
        .ok()
        .and_then(|amount| u64::try_from(amount / 100_000_000).ok())
}

fn string_field_any(value: &Value, fields: &[&str]) -> Option<String> {
    if let Some(object) = value.as_object() {
        for field in fields {
            if let Some(value) = object
                .get(*field)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                return Some(value.to_string());
            }
        }
        for nested in object.values() {
            if let Some(value) = string_field_any(nested, fields) {
                return Some(value);
            }
        }
    }
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(value) = string_field_any(item, fields) {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::channel_from_value;
    use serde_json::json;

    #[test]
    fn parses_channel_fields_from_fiber_rpc_json() {
        let channel = channel_from_value(&json!({
            "channel_id": "0xabc",
            "temporary_channel_id": "0xtmp",
            "peer_pubkey": "03peer",
            "state": "CHANNEL_READY",
            "funding_tx_hash": "0xfund"
        }));

        assert_eq!(channel.channel_id.as_deref(), Some("0xabc"));
        assert_eq!(channel.temporary_channel_id.as_deref(), Some("0xtmp"));
        assert_eq!(channel.peer_pubkey.as_deref(), Some("03peer"));
        assert_eq!(channel.funding_tx_hash.as_deref(), Some("0xfund"));
        assert!(channel.is_usable);
        assert!(!channel.is_closed);
        assert!(!channel.is_failed);
    }
}

#[cfg(test)]
mod failed_tests {
    use super::channel_from_value;
    use serde_json::json;

    #[test]
    fn detects_aborted_channel_as_failed() {
        let channel = channel_from_value(&json!({
            "channel_id": "0xabc",
            "pubkey": "03peer",
            "local_balance": "0xba43b7400",
            "state": { "state_name": "Closed", "state_flags": "FUNDING_ABORTED" }
        }));

        assert_eq!(channel.amount_ckb, Some(500));
        assert!(channel.is_closed);
        assert!(channel.is_failed);
        assert!(!channel.is_usable);
    }
}

#[cfg(test)]
mod settled_tests {
    use super::channel_from_value;
    use serde_json::json;

    #[test]
    fn detects_settled_channel_without_failure() {
        let channel = channel_from_value(&json!({
            "channel_id": "0xabc",
            "peer_pubkey": "03peer",
            "local_balance": "0x4a817c800",
            "state": "CHANNEL_CLOSED",
            "settlement_tx_hash": "0xsettle"
        }));

        assert_eq!(channel.amount_ckb, Some(200));
        assert_eq!(channel.settlement_tx_hash.as_deref(), Some("0xsettle"));
        assert!(channel.is_closed);
        assert!(!channel.is_failed);
    }
}
