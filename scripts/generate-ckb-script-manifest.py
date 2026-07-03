#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path

SCRIPTS = [
    "liquidlane-vault-lock",
    "liquidlane-vault-type",
    "liquidlane-lp-receipt-type",
    "liquidlane-capacity-request-type",
    "liquidlane-fee-claim-type",
]


def ckb_hash(data: bytes) -> str:
    digest = hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()
    return f"0x{digest}"


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate LiquidLane CKB script artifact manifest")
    parser.add_argument("--artifacts", default="ckb-scripts/build")
    parser.add_argument("--output", default="ckb-scripts/build/manifest.json")
    parser.add_argument("--network", default="testnet")
    args = parser.parse_args()

    artifacts = Path(args.artifacts)
    records = []
    for name in SCRIPTS:
        path = artifacts / name
        data = path.read_bytes()
        records.append(
            {
                "name": name,
                "path": str(path),
                "size_bytes": len(data),
                "ckb_data_hash": ckb_hash(data),
                "hash_type": "data1",
            }
        )

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"network": args.network, "scripts": records}, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
