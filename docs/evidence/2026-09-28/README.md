# Settlement reconciliation and managed provider validation

The connector and coordinator now verify native settlement independently of stale Fiber channel flags. These checks use the new production verifier, and do not reopen channels, resend closing transactions, or alter existing native stores.

## Confirmed on CKB testnet

- `merchant_offline-settlement.json`: the first recorded unilateral closure resolves through three committed transactions.
- `provider_offline-settlement.json`: the second recorded unilateral closure resolves through three committed transactions, even though the recorded provider response had no shutdown hash and remained `ShuttingDown`.
- `cooperative-settlement.json`: the recorded cooperative close resolves directly to wallet outputs in one committed closing transaction.

Each record includes the funding outpoint and complete verified transaction list. Run `liquidlane-connector settlement-status <config.json> '<funding-hash>#0'` to recheck it. A confirmation establishes historical settlement of the pinned native contracts, not the amount currently available in either wallet. The older raw native observations remain in `../2026-09-25/recovery-followup.json`; this work does not patch Fiber 0.9.0 or assert that its stale flags have disappeared.

## Managed services and browser

`provider-services.json` records an isolated, unfunded installation using the pinned official release. The check exercises real registration/signatures, fresh background heartbeats, automatic restart after killing each service, explicit service restart, and repeated setup with the same identity and keys. It verifies the signed funding address and Add capital controls in desktop and mobile Chromium. The screenshots show that live, unfunded node; there is no fabricated funding balance or marketplace response in this browser check.

Reproduce with `scripts/verify-provider-setup.py` as documented in `../../automatic-provider-setup.md`. Test services, coordinator, keys, and temporary state are removed afterward; the previous linger setting is restored. Existing participant stores and funds are not used by this check.

## Regression checks

See `validation.json` for commands and counts. Coverage includes both recorded forced-recovery graphs, incomplete or false proofs, unrelated/duplicate/uncommitted transactions, unknown output scripts, indexer lag, expired confirmations during an RPC outage and connector restart, pending native closure retaining the funding budget, pre-delivery settlement ending without a fee, and delivered fee history surviving closure. Desktop and mobile browser fixtures distinguish pending settlement from confirmed settlement despite stale native state. Those four UI fixture tests are separate from the live service/browser check above.

Still unverified: a full host reboot and a JoyID passkey-signed transfer into a freshly managed node. No new on-chain transfer or channel was requested during this validation.
