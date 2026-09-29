# Marketplace testnet evidence

Public records of native Fiber channel funding, usable receive capacity, direct fees, and recovery on CKB testnet. The checks use Fiber 0.9.0 and project-operated nodes with separate wallets, keys, and stores.

## Capacity and funding

- [Automatic capacity delivery](evidence/automatic-capacity/README.md): a signed order opened automatically within provider limits, delivered 500 CKB after a paid routed probe, collected the 61 CKB opening fee, and closed cooperatively. Includes the signed receipt, funding reference, and fee and closing transactions.
- [Provider funding](evidence/provider-funding/README.md): a confirmed 61 CKB top-up updated the signed node balance and enabled offer publication. Includes the signed transfer, balance observations, and browser screenshots.

## Availability and recovery

- [Coordinator outage](evidence/coordinator-outage/README.md): native payments continued after Core stopped. Cooperative closes and both peer-offline recovery cases produced confirmed wallet outputs. Includes the complete transaction traces, node observations, and backup verification.
- [Settlement and service recovery](evidence/settlement-recovery/README.md): the production verifier reconciled cooperative and unilateral settlement despite stale native flags. A fresh managed provider installation passed service crash/restart, identity preservation, signed heartbeat, and desktop/mobile checks.

## Verify the records

Each evidence directory includes a JSON checksum manifest. Compare its SHA-256 entries with the referenced files, inspect signed receipts, and look up the recorded transaction hashes on the CKB testnet explorer. Original observation timestamps remain in the raw records. The directory names describe the checks and stay stable across documentation updates.

Follow the [operations guide](marketplace-operations.md) to reproduce channel and payment checks, or the [provider setup guide](automatic-provider-setup.md) for managed service checks. Reproduction requires compatible nodes and testnet funds; signed transactions in these records are evidence and must not be broadcast again.

## Coverage

The [validation record](evidence/settlement-recovery/validation.json) records 88 passing Rust tests and the accompanying browser, build, and service checks. The [operations guide](marketplace-operations.md#validation-limits) also records one real JoyID purchase.

These records establish implementation behavior on project-operated nodes. Independent operator adoption, market demand, full host reboot recovery, and JoyID funding of a freshly managed node remain unverified. Testnet fees do not establish commercial pricing or revenue.
