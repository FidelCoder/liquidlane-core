# Fiber External Funding Notes

LiquidLane goal: LP vault liquidity should be the economic source for merchant receive capacity. The public product should not require a merchant or visible node operator to fund the channel.

## Verified Fiber Surface

The local Fiber node CLI exposes the external funding flow:

- `channel open_channel_with_external_funding`
- `channel submit_signed_funding_tx`
- `dev sign_external_funding_tx`

`open_channel_with_external_funding` requires:

- `--pubkey`
- `--funding-amount`
- `--shutdown-script`
- `--funding-lock-script`
- optional `--funding-lock-script-cell-deps`

`submit_signed_funding_tx` requires:

- `--channel-id`
- `--signed-funding-tx`

The node also supports `--fiber-funding-tx-shell-builder`. That builder receives a JSON payload containing the draft funding tx, the request, the CKB RPC URL, and the funding source lock script.

## Beta Mode

Current LiquidLane Core uses `open_channel` through the configured Fiber RPC. This is marked as `managed_node_beta` by `LIQUIDLANE_EXECUTOR_FUNDING_MODE`.

In beta, merchant reserve flow is:

1. Merchant signs a CKB capacity request cell.
2. Core reserves LP vault liquidity and records the lease fee.
3. LiquidLane executor submits the Fiber handoff through the configured Fiber RPC.
4. Request status becomes `requested`, `pending_fiber_channel`, `channel_open`, or `failed`.

This is an automation upgrade, not final script-level vault funding.

## Vault-Funded Target

To make true vault-funded Fiber execution, the next script iteration must let the vault authorize a Fiber funding transaction without letting the executor withdraw LP funds.

Needed work:

- Vault v2 transition for request reserve -> channel funding intent.
- Funding lock script compatible with `open_channel_with_external_funding`.
- Core builder that consumes Fiber's funding tx shell request and produces a vault-authorized CKB transaction.
- Settlement proof path for channel active, close, expiry, and release.

## Product Rule

Public users never operate nodes manually. LPs supply, merchants reserve, and LiquidLane runs the executor infrastructure.


## LiquidLane Integration Rule

Normal Fiber `open_channel` is node-wallet funded and is diagnostic only.

The product path must call `open_channel_with_external_funding`, then submit a signed funding transaction built from LiquidLane vault liquidity. If that transaction builder is not available, Core must return a retryable failure and preserve the on-chain reserve.

## Implementation Status

- Fiber RPC client methods are being wired in Core.
- Vault V2 policy models external funding as reserved -> deployed.
- The full CKB transaction builder and script deployment are the remaining critical path before claiming true vault-funded Fiber execution.
