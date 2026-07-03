# Script Audit Notes

This is an internal review of the current CKB script drafts. It is not an external security audit and does not make the scripts ready for real funds.

## Fixed In This Pass

- Replaced broad exact-instance service checks with service code-hash checks where the vault must support many receipts, requests, and claims.
- Enforced singleton-style group input/output handling for state cells.
- Switched data validation from minimum length to exact length and version checks.
- Bound vault total deltas to aggregate LP receipt supplied deltas.
- Bound vault locked-liquidity deltas to aggregate capacity request amounts.
- Bound vault fee decreases to aggregate fee claim amounts and fee increases to request fees.
- Required LP receipt buckets to exactly equal supplied balance.
- Made request amount, fee, and expiry immutable after creation.
- Made request and claim statuses monotonic.
- Rejected vault-lock outputs that keep the vault lock without the vault type.

## Still Required Before Testnet Funds

- Transaction-level tests with real CKB cells, witnesses, and invalid-path cases.
- A unique type-id style vault deployment so the vault accounting cell cannot be cloned.
- UDT-specific conservation tests for non-CKB assets.
- External audit before mainnet or user funds.
