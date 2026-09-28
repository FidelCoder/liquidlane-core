# LiquidLane Core

Fiber-native backend for CKB wallet sessions, stablecoin vault accounting, liquidity requests, Fiber channel-open orchestration, and LP fee tracking on LiquidLane.

LiquidLane turns LP stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. A user connects a CKB wallet and opens a LiquidLane wallet session.
2. LPs supply liquidity by signing a CKB vault transaction that spends the active vault cell, updates vault accounting, and mints an LP receipt cell.
3. Core records the deposit only after verifying the broadcast transaction against the active vault and receipt scripts.
4. Merchants request receive capacity and include a Fiber peer pubkey when they are ready to open a channel.
5. LiquidLane quotes lease fees and reserves available liquidity.
6. LiquidLane submits `open_channel` to a configured Fiber node. If `FIBER_RPC_URL` is missing, Core rejects the operator action and leaves the reservation unchanged.
7. Fees are only counted as earned after a request reaches `channel_open`.

## Development

```bash
cp .env.example .env
cargo run
```

The API listens on `0.0.0.0:8080` by default and stores local state in `liquidlane-data.json`.
Configure the active vault with `LIQUIDLANE_VAULT_ASSET`, `LIQUIDLANE_VAULT_CKB_ADDRESS`, and `LIQUIDLANE_CKB_NETWORK`. The beta runtime is locked to CKB testnet, and `LIQUIDLANE_VAULT_CKB_ADDRESS` must be a real `ckt1...` testnet address from the deployed vault script; Core rejects placeholder-looking values and reports the vault as unconfigured when no address is set.

## Fiber RPC

Set `FIBER_RPC_URL` to submit channel opens to a Fiber node JSON-RPC endpoint. If it is not set, LiquidLane can still supply and reserve capacity, but operator channel submission returns a clear configuration error and does not invent a channel id.

```bash
FIBER_RPC_URL=http://127.0.0.1:8227 cargo run
```

For UDT assets like USDC, requests sent to Fiber RPC must include `funding_udt_type_script`.

## Active Vault API

LiquidLane exposes the product vault that clients use for supply transactions.

```bash
curl http://localhost:8080/vault
```

## CKB Scripts

The CKB-native script source drafts live in `ckb-scripts/`. They cover vault custody, vault accounting, LP receipt cells, capacity request cells, and fee claim cells.

The current testnet deployment is recorded in `docs/testnet-deployment.md` and `ckb-scripts/deployments/`. Demo readiness is tracked in `docs/testnet-demo-readiness.md`. The scripts are still not externally audited and must not be treated as mainnet-ready.

## CKB Wallet Session API

Open a wallet session without an extra signature. Value-moving actions still require a wallet transaction proof.

```bash
curl -X POST http://localhost:8080/auth/connect \
  -H "Content-Type: application/json" \
  -d '{"ckb_address":"ckt1...","wallet_type":"joyid_ckb","role":"lp","display_name":"Atlas LP"}'
```

The signed challenge flow is still available for clients that want explicit sign-in.

Create challenge:

```bash
curl -X POST http://localhost:8080/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"ckb_address":"ckt1...","wallet_type":"joyid_ckb","role":"operator"}'
```

Verify signed CKB wallet proof:

```bash
curl -X POST http://localhost:8080/auth/verify \
  -H "Content-Type: application/json" \
  -d '{"challenge_id":"...","ckb_address":"ckt1...","wallet_type":"joyid_ckb","signature":"0x...","display_name":"Operator"}'
```

Use the returned bearer token for product APIs.

## Supply Liquidity API

Supply is a two-step CKB-native flow. Core creates a vault intent, the wallet signs a transaction that spends the active vault cell and mints an LP receipt cell, then Core settles the intent into an LP position after chain verification. Bare accounting deposits and simple transfers are rejected.

Create a supply intent:

```bash
curl -X POST http://localhost:8080/vault/supply/intents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"CKB","amount":100}'
```

Settle the intent after the wallet signs and broadcasts the vault update transaction:

```bash
curl -X POST http://localhost:8080/deposits \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"CKB","amount":100,"intent_id":"...","tx_hash":"0x...","signed_tx":{"inputs":[],"outputs":[],"witnesses":["0x..."]}}'
```

LP positions, capacity reservations, withdrawal intents, and fee-claim intents are returned by `GET /dashboard`.

For production-style settlement verification, configure `LIQUIDLANE_CKB_RPC_URL` and set `LIQUIDLANE_REQUIRE_CKB_RPC=true`. Core will reject supply settlement unless the CKB node returns the transaction as accepted.

## Tests

```bash
cargo test
scripts/check-rust-line-count.sh
```
