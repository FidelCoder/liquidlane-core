# LiquidLane Core

Backend service for vault accounting, liquidity requests, Fiber channel deployment, and fee distribution on LiquidLane.

LiquidLane turns stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. LP deposits stablecoin liquidity into a vault.
2. Merchant or wallet requests receive capacity.
3. LiquidLane quotes a lease fee and routing-fee policy.
4. Capacity is deployed into a Fiber channel.
5. Lease and routing fees are tracked back to the vault.

## Development

```bash
cp .env.example .env
cargo run
```

The API listens on `0.0.0.0:8080` by default.

```bash
curl http://localhost:8080/health
```

## Environment

- `LIQUIDLANE_BIND_ADDR`: server bind address, defaults to `0.0.0.0:8080`
- `LIQUIDLANE_ENV`: runtime environment label, defaults to `development`

## MVP API

### Vault Summary

```bash
curl "http://localhost:8080/vault?asset=USDC"
```

### Create LP Deposit

```bash
curl -X POST http://localhost:8080/deposits \
  -H "Content-Type: application/json" \
  -d '{"lp_name":"Atlas LP","asset":"USDC","amount":25000}'
```

### Quote Liquidity

```bash
curl -X POST http://localhost:8080/liquidity/quote \
  -H "Content-Type: application/json" \
  -d '{"merchant_name":"Nova Wallet","asset":"USDC","amount":10000,"duration_days":30}'
```

### Request Liquidity

```bash
curl -X POST http://localhost:8080/liquidity/requests \
  -H "Content-Type: application/json" \
  -d '{"merchant_name":"Nova Wallet","asset":"USDC","amount":10000,"duration_days":30}'
```

### Deploy Liquidity

```bash
curl -X POST http://localhost:8080/liquidity/requests/{id}/deploy
```

### Activity

```bash
curl http://localhost:8080/activity
```

## Tests

```bash
cargo test
```
