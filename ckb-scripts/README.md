# LiquidLane CKB Scripts

This folder contains the CKB-native lock/type scripts for the LiquidLane vault layer.

The current scripts are deployed on CKB testnet and wired to a live vault cell. They define the trust layer so Core and the app do not rely on a normal wallet address for pooled funds.

## Services Covered

- `vault-lock`: guards custody cells, requires a real service path or admin path, and rejects vault-lock outputs without the vault type.
- `vault-type`: validates singleton aggregate vault accounting and only allows deltas through the matching service script.
- `lp-receipt-type`: tracks LP supplied, available, reserved, deployed, and claimed balances with LP/request/claim transition rules.
- `capacity-request-type`: tracks merchant capacity requests with immutable amount/fee/expiry and monotonic status changes.
- `fee-claim-type`: validates LP fee claim cells with immutable amount and monotonic status changes.
- `shared`: small no-std helpers for argument parsing, hash checks, data reads, and capacity scans.

## Script Arguments

All script arguments are raw 32-byte hashes packed in order. Vault references use exact script hashes. Service-family references use script code hashes so one vault can work with many receipt, request, and claim cells.

| Script | Args |
| --- | --- |
| `vault-lock` | admin lock hash, vault type script hash, LP receipt code hash, request code hash, fee claim code hash |
| `vault-type` | admin lock hash, LP receipt code hash, request code hash, fee claim code hash |
| `lp-receipt-type` | vault type script hash, LP lock hash, request code hash, fee claim code hash, asset id, position id |
| `capacity-request-type` | vault type script hash, merchant lock hash, operator lock hash, request id |
| `fee-claim-type` | vault type script hash, LP receipt type script hash, LP lock hash, claim id |

## Deployment

Deployment records live in `ckb-scripts/deployments/`. Local builds only have artifact hashes; public confirmation requires CKB testnet transaction hashes and cell out-points.

Build VM-safe RISC-V artifacts with:

```bash
scripts/setup-riscv-toolchain.sh
export RISCV_TOOLCHAIN_BIN=/tmp/liquidlane-riscv-toolchain/root/usr/bin
scripts/build-ckb-scripts.sh
```

Current testnet records:

- Script deployment: `ckb-scripts/deployments/testnet-2026-07-04-a00be7fdb859.json`
- Vault cell: `ckb-scripts/deployments/vault-testnet-2026-07-04-477be93d5587.json`
- Script tx: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f`
- Vault tx: `0x477be93d5587b6ff040858605a0e2c440f6a2e3587fa1bd3dd139391e06b2370`
