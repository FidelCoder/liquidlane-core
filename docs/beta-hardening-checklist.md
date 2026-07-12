# LiquidLane Beta Hardening Checklist

This checklist keeps the CKB testnet beta deployable without local servers or hidden manual steps. Do not place private keys or auth tokens in this file.

## Render Core Environment

Required:

- `LIQUIDLANE_ENV=production`
- `LIQUIDLANE_BIND_ADDR=0.0.0.0:10000`
- `LIQUIDLANE_CKB_NETWORK=testnet`
- `LIQUIDLANE_CKB_RPC_URL=<CKB testnet RPC>`
- `LIQUIDLANE_REQUIRE_CKB_RPC=true`
- `LIQUIDLANE_CORS_ALLOWED_ORIGIN=https://liquidlane-app.vercel.app`
- `LIQUIDLANE_VAULT_CKB_ADDRESS=<active vault address>`
- `LIQUIDLANE_VAULT_CELL_OUT_POINT=<active vault out-point>`
- `LIQUIDLANE_VAULT_LOCK_CODE_HASH=<deployed script hash>`
- `LIQUIDLANE_VAULT_LOCK_OUT_POINT=<deployed script out-point>`
- `LIQUIDLANE_VAULT_TYPE_CODE_HASH=<deployed script hash>`
- `LIQUIDLANE_VAULT_TYPE_OUT_POINT=<deployed script out-point>`
- `LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=<deployed script hash>`
- `LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=<deployed script out-point>`
- `LIQUIDLANE_REQUEST_TYPE_CODE_HASH=<deployed script hash>`
- `LIQUIDLANE_REQUEST_TYPE_OUT_POINT=<deployed script out-point>`
- `FIBER_RPC_URL=<managed Fiber node RPC>`
- `FIBER_RPC_AUTH_TOKEN=<Fiber RPC bearer token when enabled>`

Recommended:

- `RUST_LOG=liquidlane_core=info,tower_http=info`
- `LIQUIDLANE_EXECUTOR_ENABLED=true`
- `LIQUIDLANE_EXECUTOR_POLL_INTERVAL_MS=5000`
- `LIQUIDLANE_EXECUTOR_MAX_RETRIES=3`
- persistent Render disk mounted at the path used by `LIQUIDLANE_DATA_PATH`

## Vercel Frontend Environment

Required:

- `NEXT_PUBLIC_LIQUIDLANE_CORE_URL=<Render Core HTTPS URL>`
- `NEXT_PUBLIC_CKB_NETWORK=testnet`
- `NEXT_PUBLIC_CKB_RPC_URL=<CKB testnet RPC>`
- `NEXT_PUBLIC_PUDGE_EXPLORER_URL=https://pudge.explorer.nervos.org`
- JoyID cell dep values for testnet signing.

## Monitoring

Public safe checks:

- `GET /health` shows RPC, vault, and executor readiness.
- `GET /monitoring` shows beta readiness, executor state, and safe state counts.

Internal operator checks:

- `GET /internal/executor/health`
- `GET /internal/executor/jobs`
- `POST /internal/executor/jobs/{id}/retry`
- `POST /internal/executor/release-expired`
- `GET /internal/state/export` returns a safe backup summary, not secrets or auth tokens.

## Smoke Tests

Run after every production deploy:

1. Open the Vercel app and connect JoyID.
2. Confirm wallet session survives refresh.
3. Supply a small CKB amount above the active minimum.
4. Confirm the success receipt shows tx hash and Pudge explorer link.
5. Confirm LP vault balance refreshes.
6. Withdraw part of the LP balance.
7. Confirm withdrawal tx hash and refreshed balance.
8. Reserve merchant capacity below available vault liquidity.
9. Confirm reserve tx hash and queue entry.
10. Confirm `/monitoring` shows updated request and executor counts.

## Beta Guardrails

- Keep the app testnet-only until the v2 scripts are audited and redeployed.
- Never ask beta users to fund node wallets. LP vault liquidity is the source of merchant capacity.
- Failed Fiber handoffs must stay retryable and visible.
- Expired reservations must return to LP availability through the worker or internal release endpoint.
- Every user-facing on-chain action must show success/failure, amount, tx hash, and explorer link.
