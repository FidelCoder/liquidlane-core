# LiquidLane Core

Backend service for wallet-authenticated vault accounting, liquidity requests, Fiber channel deployment tracking, and fee distribution on LiquidLane.

LiquidLane turns stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. A user connects a wallet and signs a LiquidLane challenge.
2. LPs deposit stablecoin liquidity into the vault.
3. Merchants request receive capacity.
4. LiquidLane quotes lease fees and reserves available liquidity.
5. Capacity is deployed into a Fiber channel record.
6. Lease fees are tracked back to the vault.

## Development

```bash
cp .env.example .env
cargo run
```

The API listens on `0.0.0.0:8080` by default and stores local state in `liquidlane-data.json`.

## Wallet Auth API

Create challenge:

```bash
curl -X POST http://localhost:8080/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"wallet_address":"0x...","role":"operator"}'
```

Verify signed message:

```bash
curl -X POST http://localhost:8080/auth/verify \
  -H "Content-Type: application/json" \
  -d '{"challenge_id":"...","wallet_address":"0x...","signature":"0x...","display_name":"Operator"}'
```

Use the returned bearer token for product APIs.

## Tests

```bash
cargo test
```
