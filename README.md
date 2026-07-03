# LiquidLane Core

Fiber-native backend for CKB wallet sessions, stablecoin vault accounting, liquidity requests, Fiber channel-open orchestration, and LP fee tracking on LiquidLane.

LiquidLane turns LP stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. A user connects a CKB wallet and signs a LiquidLane challenge.
2. LPs record stablecoin liquidity into the vault.
3. Merchants request receive capacity and include a Fiber peer pubkey when they are ready to open a channel.
4. LiquidLane quotes lease fees and reserves available liquidity.
5. LiquidLane submits `open_channel` to a configured Fiber node, or marks the request as `pending_fiber_channel` when no node is configured.
6. Fees are only counted as earned after a request reaches `channel_open`.

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

## CKB Wallet Auth API

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

## Tests

```bash
cargo test
```
