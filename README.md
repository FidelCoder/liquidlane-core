# LiquidLane Core

Fiber-native backend for CKB wallet sessions, stablecoin vault accounting, liquidity requests, Fiber channel-open orchestration, and LP fee tracking on LiquidLane.

LiquidLane turns LP stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. A user connects a CKB wallet and opens a LiquidLane wallet session.
2. LPs supply liquidity by confirming a CKB wallet transaction to the vault.
3. Core records the deposit only after receiving the signed transaction proof.
4. Merchants request receive capacity and include a Fiber peer pubkey when they are ready to open a channel.
5. LiquidLane quotes lease fees and reserves available liquidity.
6. LiquidLane submits `open_channel` to a configured Fiber node, or marks the request as `pending_fiber_channel` when no node is configured.
7. Fees are only counted as earned after a request reaches `channel_open`.

## Development

```bash
cp .env.example .env
cargo run
```

The API listens on `0.0.0.0:8080` by default and stores local state in `liquidlane-data.json`.

## Fiber RPC

Set `FIBER_RPC_URL` to submit channel opens to a Fiber node JSON-RPC endpoint. If it is not set, LiquidLane still reserves capacity but keeps the request in `pending_fiber_channel` without inventing a channel id.

```bash
FIBER_RPC_URL=http://127.0.0.1:8227 cargo run
```

For UDT assets like USDC, requests sent to Fiber RPC must include `funding_udt_type_script`.

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

`POST /deposits` requires `signed_tx` from the CKB wallet. Bare accounting deposits are rejected.

```bash
curl -X POST http://localhost:8080/deposits \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"CKB","amount":100,"tx_hash":"0x...","signed_tx":{"inputs":[],"outputs":[],"witnesses":["0x..."]}}'
```

## Tests

```bash
cargo test
```
