# Vault State Machine

LiquidLane Core models the CKB-native vault lifecycle that the app and deployed CKB scripts enforce.

## Supply

1. LP opens `POST /vault/supply/intents` with asset and amount.
2. Core returns the active vault address, receipt cell id, memo, expiry, and pending intent id.
3. Wallet signs and broadcasts the CKB transaction to the vault.
4. Client settles with `POST /deposits` using `intent_id`, `tx_hash`, and `signed_tx`.
5. Core creates an LP position with available balance and receipt cell reference.

## Capacity Request

1. Merchant quotes or submits a capacity request.
2. Core checks live LP position availability.
3. Core reserves position balances and creates a request-cell reference.
4. Fiber channel open is submitted through the configured Fiber RPC. Missing RPC config rejects the operator action before state mutation.
5. Successful submit moves reserved balance to deployed balance. Failed submit releases it.

## Withdrawals

1. LP creates `POST /vault/withdrawals/intents` for an active position and amount.
2. Core returns the receipt cell reference and memo for the wallet transaction.
3. Client settles with `POST /vault/withdrawals/{id}/settle` using `tx_hash` and `signed_tx`.
4. Core reduces the LP position and closes it when the supplied amount reaches zero.

## Fee Claims

1. LP creates `POST /vault/fees/claims` for earned but unclaimed position fees.
2. Wallet signs and broadcasts the fee-claim transaction that spends the receipt and vault cells.
3. Client settles with `POST /vault/fees/claims/{id}/settle` using `tx_hash`, `receipt_cell_out_point`, and `signed_tx`.
4. Core verifies the vault delta, fee-claim cell, and updated receipt before marking fees claimed.

## Script Layer

CKB does not use EVM contracts. The production trust layer is expressed as lock/type scripts for:

- vault custody
- LP receipt cells
- merchant request cells
- fee claim cells

Script sources live in `ckb-scripts/`. They enforce singleton-style group transitions, strict data lengths, and service-specific accounting deltas. The current scripts are deployed on CKB testnet for product testing, but they still need external audit and broader transaction-level coverage before protecting real funds.

Core exposes script code-hash configuration so clients can display and validate which script family protects the active vault once those scripts are deployed.
