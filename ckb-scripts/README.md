# LiquidLane CKB Scripts

This folder contains the CKB-native lock/type script source drafts for the LiquidLane vault layer.

These scripts are not deployed, audited, or wired to a testnet cell yet. They define the intended trust layer so Core and the app stop relying on a normal wallet address for pooled funds.

## Services Covered

- `vault-lock`: guards custody cells and only allows spends through admin, LP receipt, capacity request, or fee claim paths.
- `vault-type`: validates aggregate vault accounting and prevents unauthorized value movement.
- `lp-receipt-type`: tracks LP supplied, available, reserved, deployed, and claimed balances.
- `capacity-request-type`: tracks merchant capacity requests and requires merchant or operator authorization.
- `fee-claim-type`: validates LP fee claim cells against the LP receipt and vault path.
- `shared`: small no-std helpers for argument parsing, hash checks, data reads, and capacity scans.

## Script Arguments

All script arguments are raw 32-byte hashes packed in order.

| Script | Args |
| --- | --- |
| `vault-lock` | admin lock hash, vault type hash, LP receipt type hash, request type hash, fee claim type hash |
| `vault-type` | admin lock hash, LP receipt type hash, request type hash, fee claim type hash |
| `lp-receipt-type` | vault type hash, LP lock hash, asset id, position id |
| `capacity-request-type` | vault type hash, merchant lock hash, operator lock hash, request id |
| `fee-claim-type` | vault type hash, LP receipt type hash, LP lock hash, claim id |

## Deployment Path

1. Compile each script to a RISC-V CKB binary with the CKB script toolchain.
2. Run unit tests and transaction-level tests with generated cells and witnesses.
3. Deploy script binaries to testnet cells.
4. Set the resulting code hashes in Core:

```bash
LIQUIDLANE_VAULT_LOCK_CODE_HASH=0x...
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0x...
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x...
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x...
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=0x...
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1...
```

Until that deployment is done, Core should keep reporting the vault as unconfigured instead of showing a placeholder vault address.
