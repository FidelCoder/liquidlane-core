# LiquidLane V1 Vault Capability Audit

This audit keeps the product flow honest: LP vault liquidity must fund merchant Fiber capacity. Normal Fiber node-wallet funding is diagnostic only.

## Audit Result

The current deployed v1 vault can support supply, withdrawal, and merchant reserve accounting, but it is not sufficient for final vault-funded Fiber external funding.

V2 is required before LiquidLane can move reserved vault liquidity into a Fiber funding lock as product success.

## Why V1 Is Not Enough

The v1 flow can prove these actions:

- LP adds CKB to the active vault.
- LP receives a receipt-backed position.
- LP withdraws only available receipt liquidity.
- Merchant reserve creates a request cell.
- Vault accounting moves liquidity from available to reserved.

The missing transition is the critical one:

```txt
reserved vault liquidity -> negotiated Fiber funding lock
```

For product safety, that transition must prove all of the following on-chain:

- The request cell exists and matches the reserve.
- The funding amount equals the reserved request amount.
- The Fiber funding lock matches the external funding negotiation.
- The executor cannot redirect funds to an arbitrary lock.
- LP receipt accounting remains consistent.
- Reserved/deployed funds cannot be withdrawn by LPs until release or settlement rules allow it.

V1 does not carry enough funding-intent proof data to enforce that full transition safely.

## Transition Matrix

| Transition | V1 status | Product decision |
| --- | --- | --- |
| Supply | Supported | Keep working while v2 is prepared |
| Withdraw available liquidity | Supported | Keep working |
| Merchant reserve | Supported | Keep working |
| Prepare Fiber external funding | Partial off-chain/Core support | Needs v2 script-backed proof |
| Submit vault-funded Fiber tx | Not complete in v1 | Requires v2 |
| Mark channel open | Watcher shell exists | Requires real funding tx and Fiber active channel |
| Failed funding repair | Partial Core state exists | Needs v2 release/funding rules |
| Expired release | Partial Core state exists | Needs on-chain v2 release transaction |
| Fee claim | Basic receipt claim exists | Needs final fee/yield accounting |
| Channel close/settle | Not complete | Requires Fiber close/settlement integration |

## Required V2 Capabilities

V2 must add or enforce:

- Request/funding intent proof data.
- Authorized external funding transition.
- Exact amount matching.
- Exact Fiber funding lock matching.
- Reserved to deployed accounting.
- Safe retry/release for failed funding.
- Settlement path back from deployed liquidity.

## Active Config Check

Core must treat the active vault as product-ready only when:

- active vault cell is configured,
- v2 script code hashes/out-points are configured,
- external funding transaction builder is enabled,
- Fiber RPC supports external funding,
- CKB RPC dry-run passes the funding transaction.

Until then, merchant requests should stop at `funding_required` after the reserve tx confirms.

## Product Rule

A merchant request is not usable capacity until both are true:

1. the CKB vault-funded Fiber funding tx is confirmed, and
2. Fiber reports the channel active/usable.
