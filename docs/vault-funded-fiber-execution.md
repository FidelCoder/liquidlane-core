# Vault-Funded Fiber Execution

LiquidLane's product flow is LP-vault funded capacity:

1. LPs supply CKB into the LiquidLane vault.
2. Merchants reserve receive capacity against live vault liquidity and pay the lease fee in the reserve transaction.
3. The vault marks the requested amount as reserved.
4. Core calls the managed Fiber node to open the channel.
5. Fiber invokes `FIBER_FUNDING_TX_SHELL_BUILDER`.
6. The shell builder posts Fiber's funding payload to Core at `/internal/fiber/funding-builder`.
7. Core builds a CKB transaction that spends the reserved vault cell into Fiber's funding lock, marks the request deployed, and creates a funding-intent proof cell.
8. Fiber signs the executor lock group, broadcasts the funding transaction, then reports channel state.
9. Core's watcher moves the request from funding submitted/pending to channel open only when Fiber reports an active channel.

## Funding Source

The large channel amount comes from LP vault liquidity. The managed Fiber node wallet is only used for executor signing, funding-intent cell capacity, and transaction fees. It must not be treated as merchant liquidity.

## Required Mode

```txt
LIQUIDLANE_FIBER_FUNDING_MODE=vault_external
LIQUIDLANE_VAULT_FUNDING_BUILDER_ENABLED=true
LIQUIDLANE_VAULT_FUNDING_SIGNER_ENABLED=true
```

Fiber node env must include either:

```txt
LIQUIDLANE_CORE_FUNDING_BUILDER_URL=https://<core-host>/internal/fiber/funding-builder
```

or a custom:

```txt
FIBER_FUNDING_TX_SHELL_BUILDER=curl -fsS -H content-type:application/json --data-binary @- https://<core-host>/internal/fiber/funding-builder
```

## Script Requirements

The funding transaction is valid only when the scripts can prove:

- request amount equals Fiber local funding amount
- vault reserved decreases by the request amount
- vault deployed increases by the same amount
- vault physical capacity decreases only into the negotiated Fiber funding lock
- request cell moves to deployed under executor/vault authority
- funding-intent proof cell exists for the same funding lock and request

## Safety Rule

If the builder URL or v2 script values are missing, Core keeps the request repairable and does not fall back to node-wallet liquidity.
