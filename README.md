# LiquidLane Core

Backend service for vault accounting, authenticated liquidity requests, Fiber channel deployment tracking, and fee distribution on LiquidLane.

LiquidLane turns stablecoin liquidity into on-demand Fiber payment-channel capacity for wallets, merchants, and apps.

## Product Flow

1. A user signs in as an LP, merchant, or operator.
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

## Environment

- `LIQUIDLANE_BIND_ADDR`: server bind address, defaults to `0.0.0.0:8080`
- `LIQUIDLANE_ENV`: runtime environment label, defaults to `development`
- `LIQUIDLANE_DATA_PATH`: local JSON state path, defaults to `./liquidlane-data.json`

## API Quickstart

Create or resume a user session:

```bash
curl -X POST http://localhost:8080/auth/start \
  -H "Content-Type: application/json" \
  -d '{"name":"Atlas LP","email":"atlas@liquidlane.local","role":"lp"}'
```

Use the returned token:

```bash
TOKEN="returned-token"
```

Get dashboard:

```bash
curl http://localhost:8080/dashboard \
  -H "Authorization: Bearer $TOKEN"
```

Create LP deposit:

```bash
curl -X POST http://localhost:8080/deposits \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"USDC","amount":25000}'
```

Quote liquidity as a merchant/operator:

```bash
curl -X POST http://localhost:8080/liquidity/quote \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"USDC","amount":10000,"duration_days":30}'
```

Request liquidity:

```bash
curl -X POST http://localhost:8080/liquidity/requests \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"asset":"USDC","amount":10000,"duration_days":30}'
```

Deploy liquidity:

```bash
curl -X POST http://localhost:8080/liquidity/requests/{id}/deploy \
  -H "Authorization: Bearer $TOKEN"
```

## Tests

```bash
cargo test
```
