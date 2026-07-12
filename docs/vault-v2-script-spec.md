# LiquidLane Vault v2 Script Spec

Status: implementation target for vault-funded Fiber execution. The deployed testnet v1 scripts remain active until v2 is compiled, deployed, and the frontend/Core env points to the v2 out-points.

## Goals

- LP funds are held by the LiquidLane vault script, not by a public node-operator wallet.
- Merchants reserve receive capacity with a signed CKB request cell.
- LiquidLane executor may move reserved liquidity into Fiber funding, but cannot withdraw LP funds.
- Expired or failed requests can release liquidity back to LP withdrawable balance.
- LP receipts and vault totals must stay balanced.

## Data Model

### Vault Cell

Fields:

- `version`
- `total`
- `available`
- `reserved`
- `deployed`
- `fee_balance`
- `executor_key_hash`

Invariant:

`total = available + reserved + deployed`

### LP Receipt Cell

Fields:

- `version`
- `supplied`
- `available`
- `reserved`
- `deployed`
- `earned`
- `claimed`

Invariants:

- `supplied = available + reserved + deployed`
- `earned >= claimed`

### Capacity Request Cell

Fields:

- `version`
- `merchant_lock_hash`
- `amount`
- `lease_fee`
- `expiry`
- `fiber_peer_hash`
- `status`

Statuses:

- `reserved`
- `opening`
- `active`
- `failed`
- `expired`
- `released`

## Allowed Transitions

### Supply

Inputs:

- previous vault cell
- LP wallet funding cells

Outputs:

- updated vault cell
- LP receipt cell

Rules:

- vault `total` increases by amount
- vault `available` increases by amount
- new LP receipt `supplied` and `available` equal amount

### Withdraw Available

Inputs:

- vault cell
- LP receipt cell
- LP wallet signature

Outputs:

- updated vault cell
- updated LP receipt cell or receipt close
- LP payout cell

Rules:

- only `available` can be withdrawn
- reserved and deployed liquidity cannot be withdrawn
- vault and receipt deltas must match

### Reserve Capacity

Inputs:

- vault cell
- LP receipts selected by Core
- merchant request cell
- merchant wallet signature

Outputs:

- updated vault cell
- updated LP receipts
- request cell in `reserved`

Rules:

- request amount cannot exceed vault `available`
- vault `available` decreases by amount
- vault `reserved` increases by amount
- lease fee increases `fee_balance`
- LP receipts receive proportional reserved amount and fee share

### Execute/Open Fiber

Inputs:

- vault cell
- reserved request cell
- executor signature
- Fiber funding intent/proof cell

Outputs:

- updated vault cell
- request cell in `opening` or `active`

Rules:

- executor key hash must match vault config
- executor cannot reduce `total`
- reserved decreases by amount
- deployed increases by amount

### Release Expired

Inputs:

- vault cell
- expired request cell

Outputs:

- updated vault cell
- request cell `released` or closed

Rules:

- current timestamp must be >= request expiry
- reserved decreases by amount
- available increases by amount
- no executor withdrawal is allowed

### Settle / Close

Inputs:

- vault cell
- active request/channel proof
- executor settlement proof

Outputs:

- updated vault cell
- request closed/released

Rules:

- deployed decreases by settled amount
- available increases by returned liquidity
- loss/recovery policy must be explicit before mainnet

### Claim Fees

Inputs:

- vault cell
- LP receipt cell
- LP wallet signature

Outputs:

- updated vault cell
- updated LP receipt cell
- LP fee payout cell

Rules:

- claim amount cannot exceed `earned - claimed`
- vault `fee_balance` decreases by claim amount

## Security Assumptions

- Executor authorization is limited to reserve -> opening/active and settlement transitions.
- Executor cannot withdraw LP funds or claim LP fees.
- Merchant cannot withdraw vault funds; merchant only creates request cells and receives Fiber capacity.
- Any failed Fiber handoff remains reserved until explicit release/expiry path runs.

## Current Beta Gap

Core currently supports `managed_node_beta` execution through Fiber RPC. True `external_funding` requires the v2 funding transaction builder and deployed v2 scripts.
