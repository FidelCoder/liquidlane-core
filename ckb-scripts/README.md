# LiquidLane CKB Scripts

This folder contains the CKB-native lock/type script source drafts for the LiquidLane vault layer.

These scripts are not deployed, audited, or wired to a testnet cell yet. They define the intended trust layer so Core and the app stop relying on a normal wallet address for pooled funds.

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

## Deployment Path

1. Compile each script to a RISC-V CKB binary with the CKB script toolchain.
2. Add transaction-level tests for create, update, close, invalid duplicate groups, bad actor paths, and bad accounting deltas.
3. Deploy a unique vault instance using CKB's type-id style pattern so the vault accounting cell cannot be cloned with the same args.
4. Deploy script binaries to testnet cells.
5. Set the resulting code hashes in Core:

```bash
LIQUIDLANE_VAULT_LOCK_CODE_HASH=0x...
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0x...
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x...
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x...
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=0x...
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1...
```

Until that deployment is done, Core should keep reporting the vault as unconfigured instead of showing a placeholder vault address.
