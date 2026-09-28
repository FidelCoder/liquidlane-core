#!/usr/bin/env python3
"""Trace confirmed native commitments into participant wallet outputs (read-only)."""
import argparse
import datetime
import functools
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("commitments", type=Path, help="Recorded get_transaction results under transactions")
parser.add_argument("provider_node_info", type=Path)
parser.add_argument("merchant_node_info", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
locks = {
    "provider": json.loads(args.provider_node_info.read_text())["default_funding_lock_script"],
    "merchant": json.loads(args.merchant_node_info.read_text())["default_funding_lock_script"],
}
transactions = {}


@functools.lru_cache(maxsize=1024)
def request(method, encoded_params):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": json.loads(encoded_params)})
    result = json.loads(subprocess.check_output([
        "curl", "--fail", "--silent", "--show-error", "--max-time", "30",
        "--url", "https://testnet.ckb.dev/rpc", "-H", "Content-Type: application/json", "--data", payload,
    ]))
    if result.get("error"):
        raise RuntimeError(result["error"])
    return result["result"]


def rpc(method, params):
    return request(method, json.dumps(params, sort_keys=True))


def transaction(tx_hash):
    result = rpc("get_transaction", [tx_hash])
    if not result or result["tx_status"]["status"] != "committed":
        raise RuntimeError(f"Transaction is not committed: {tx_hash}")
    return result


def owner(output):
    return next((role for role, lock in locks.items() if output["lock"] == lock), None)


def spender(outpoint, lock):
    search = {"script": lock, "script_type": "lock", "script_search_mode": "exact"}
    cursor = None
    for _ in range(20):
        params = [search, "asc", "0x64"] + ([cursor] if cursor else [])
        page = rpc("get_transactions", params)
        for item in page["objects"]:
            if item["io_type"] != "input":
                continue
            candidate = transaction(item["tx_hash"])
            if any(i["previous_output"] == outpoint for i in candidate["transaction"]["inputs"]):
                return item["tx_hash"], candidate
        if not page["objects"]:
            return None
        if page["last_cursor"] == cursor:
            raise RuntimeError("Indexer cursor did not advance")
        cursor = page["last_cursor"]
    raise RuntimeError("Recovery history exceeds inspection limit")


def trace(outpoint, depth=0):
    if depth > 8:
        raise RuntimeError("Recovery has more than eight contract transitions")
    tx = transaction(outpoint["tx_hash"])["transaction"]
    index = int(outpoint["index"], 16)
    output = tx["outputs"][index]
    live = rpc("get_live_cell", [outpoint, False])["status"]
    role = owner(output)
    item = {"outpoint": outpoint, "owner": role, "capacity_shannons": int(output["capacity"], 16), "live_status": live}
    if role:
        if output["type"] is not None or tx["outputs_data"][index] != "0x":
            raise RuntimeError("Wallet output contains a type script or data")
        return {"wallet_outputs": [item], "pending_outputs": []}
    if live == "live":
        return {"wallet_outputs": [], "pending_outputs": [item]}
    spending = spender(outpoint, output["lock"])
    if spending is None:
        raise RuntimeError(f"No confirmed spender found for {outpoint}")
    tx_hash, result = spending
    transactions[tx_hash] = result
    print(f"{outpoint['tx_hash']}#{index} → {tx_hash}", flush=True)
    combined = {"wallet_outputs": [], "pending_outputs": []}
    for next_index in range(len(result["transaction"]["outputs"])):
        branch = trace({"tx_hash": tx_hash, "index": hex(next_index)}, depth + 1)
        for key in combined:
            combined[key].extend(branch[key])
    return combined


if rpc("get_block_hash", ["0x0"]) != "0x10639e0895502b5688a6be8cf69460d76541bfa4821629d86d62ba0aae3f9606":
    raise SystemExit("Expected CKB testnet genesis")
roots = json.loads(args.commitments.read_text())["transactions"]
recoveries = {tx_hash: trace({"tx_hash": tx_hash, "index": "0x0"}) for tx_hash in roots}
# Native settlement may merge existing wallet cells to pay fees. Count their
# inputs too, and count each settlement transaction once across both recoveries.
net = dict.fromkeys(locks, 0)
for result in transactions.values():
    tx = result["transaction"]
    for output in tx["outputs"]:
        role = owner(output)
        if role:
            net[role] += int(output["capacity"], 16)
    for cell_input in tx["inputs"]:
        previous = cell_input["previous_output"]
        output = transaction(previous["tx_hash"])["transaction"]["outputs"][int(previous["index"], 16)]
        role = owner(output)
        if role:
            net[role] -= int(output["capacity"], 16)
report = {
    "observed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "recoveries": recoveries,
    "transactions": transactions,
    "net_wallet_recovery_shannons": net,
    "all_contract_outputs_resolved": all(not r["pending_outputs"] for r in recoveries.values()),
    "note": "Confirmed wallet arrivals may later be spent. Net recovery deducts additional wallet inputs used by native settlement; commitment transaction fees were incurred earlier.",
}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: report[k] for k in ["all_contract_outputs_resolved", "net_wallet_recovery_shannons"]}))
