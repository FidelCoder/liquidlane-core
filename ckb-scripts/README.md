# LiquidLane CKB Scripts

This folder preserves the legacy vault's CKB lock/type scripts and public deployment records. The marketplace uses native Fiber channels; these scripts belong to the separate legacy runtime.

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

## V2 Funding Artifact

`funding-intent-type` is the first deployable artifact for the vault-funded Fiber path. It binds a reserved request, executor authority, and expected Fiber funding lock so Core can distinguish real LP-vault funding from node-wallet diagnostic funding.

The full v2 rollout still requires the active vault/request/receipt scripts and vault cell to be migrated or freshly deployed with `LIQUIDLANE_VAULT_SCRIPT_VERSION=v2`.

## Deployment

Deployment records live in `ckb-scripts/deployments/`. Local builds only have artifact hashes; public confirmation requires CKB testnet transaction hashes and cell out-points.

Build VM-safe RISC-V artifacts with:

```bash
scripts/setup-riscv-toolchain.sh
export RISCV_TOOLCHAIN_BIN=/tmp/liquidlane-riscv-toolchain/root/usr/bin
scripts/build-ckb-scripts.sh
```

Public script and vault records are indexed in [deployments](deployments/README.md). Transaction hashes identify the records; inspect their current on-chain state before using any historical outpoint.

## Legacy runtime and recovery

Legacy mode must be selected explicitly in the process environment with `LIQUIDLANE_PRODUCT_MODE=legacy_vault`. It reads its original `.env` and JSON state. `.env.legacy.example` in the repository root documents the required configuration. Existing ledgers, deployed cells, and native node stores must be reconciled separately from marketplace provider balances.

Keep existing private state and keys during recovery. The legacy API exposes the configured vault through `/vault` and recorded positions through `/dashboard`; configuration alone does not establish that a cell remains live or a liability has been settled. See the [operations guide](../docs/marketplace-operations.md#legacy-boundary) for the runtime boundary and the [script review](AUDIT.md) for outstanding security work.
