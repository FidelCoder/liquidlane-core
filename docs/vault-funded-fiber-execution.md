# Vault-Funded Fiber Execution

LiquidLane's product flow is LP-vault funded capacity:

1. LPs supply CKB into the LiquidLane vault.
2. Merchants reserve receive capacity against the vault.
3. The vault marks the requested amount as reserved.
4. LiquidLane negotiates Fiber external funding.
5. A CKB funding transaction moves the reserved vault liquidity into the Fiber funding lock.
6. Fiber accepts the signed funding transaction and reports an active channel.
7. Vault accounting moves from reserved to deployed.

## Current v1 Reality

The deployed v1 flow can prove supply, withdrawal, and merchant reservation on CKB testnet.

It must not be represented as complete vault-funded Fiber execution because the current executor path historically used Fiber's normal `open_channel` RPC. Normal `open_channel` funds the channel from the Fiber node wallet.

That node-wallet path is now diagnostic only.

## Correct Executor Mode

Product mode is:

```txt
LIQUIDLANE_FIBER_FUNDING_MODE=vault_external
```

Diagnostic mode is:

```txt
LIQUIDLANE_FIBER_FUNDING_MODE=node_wallet_diagnostic
```

Legacy `managed_node_beta` is normalized to `node_wallet_diagnostic`.

## External Funding Requirements

Core must use Fiber's external funding flow:

- `open_channel_with_external_funding`
- build a LiquidLane vault-funded CKB transaction
- `submit_signed_funding_tx`
- watch CKB and Fiber channel state

The funding transaction must spend only reserved vault liquidity and must create the Fiber funding-lock output expected by the negotiated channel.

## Script Requirements

The vault scripts must enforce:

- merchant request amount equals external funding amount
- reserved liquidity moves to deployed only for the matching request
- LPs cannot withdraw reserved/deployed liquidity
- executor cannot redirect funds to an arbitrary lock
- failed funding is retryable or releasable by policy

## Beta Safety Rule

If `vault_external` mode is selected and the external funding transaction builder is not ready, Core must fail the handoff safely with a clear message. It must not silently fall back to node-wallet liquidity.
