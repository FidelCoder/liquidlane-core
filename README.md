# LiquidLane

A CKB testnet marketplace for **initial Fiber receive capacity**. Providers operate their own nodes, fund their own native bidirectional channels, and retain their keys. LiquidLane coordinates discovery, signed quotes, orders, delivery evidence, and direct service fees.

The first pilot charges for opening capacity. It does not guarantee duration, ongoing replenishment, or passive yield. No pooled vault or custom contract is required by this flow.

## Run locally

Requires Rust 1.91 and a CKB testnet RPC with an indexer. Participant connectors use Fiber **0.9.0**; automatic provider setup installs it on Linux x86_64.

```sh
cp .env.example .env.marketplace
cargo build --locked --bins
cargo run --locked --bin liquidlane-core
```

Marketplace loads `.env.marketplace`; the legacy `.env` is read only when `LIQUIDLANE_PRODUCT_MODE=legacy_vault` is explicitly set in the process environment. Marketplace is the default product mode and uses a separate SQLite database. The coordinator never needs Fiber RPC access or a private key.

The web app is in the sibling `liquidlane-app` repository. Connect JoyID and sign in. Merchants use **Request Capacity → My receiving nodes**; providers use **Supply Liquidity → My offers & nodes**. Generate a pairing code and download the pairing file, then run connector setup on the node machine to create its local configuration. A receiving node needs its native reserve plus CKB for change and network fees; it is not an empty wallet checkout.

```sh
cargo install --locked --path . --bin liquidlane-connector
liquidlane-connector setup ./liquidlane-pairing.json
```

For a new provider node on Linux x86_64, use `liquidlane-connector setup ./liquidlane-pairing.json --new-node --background`. This creates the local node and starts Fiber and its connector as user services. The UI moves to Add capital after a fresh check-in. For an existing node, use `--background` without `--new-node`. See [automatic provider setup](docs/automatic-provider-setup.md) for the local launcher, security boundaries, and service controls.

Provider setup enables automatic public requests. After connecting, **Add capital** displays the node-signed funding address and asks your wallet to transfer CKB there. A 500 CKB offer needs at least 663 CKB available before existing reservations. Set the node's per-order and total channel funding limits and minimum fee, then publish an offer. Any merchant can choose it; no merchant allowlist or per-order approval is needed. The connector checks both signatures and local limits before funding. Fees are paid after delivery and can remain unpaid. Existing restricted/manual configurations retain their policy until their operator explicitly updates them.

## Flow

1. Merchant chooses an online provider and requests capacity.
2. Provider connector signs the quote after checking local policy.
3. Merchant signs the quote hash; the provider connector authorizes funding under its local policy.
4. Provider node opens a native bidirectional channel using its own wallet.
5. Both connectors report the channel; Core checks its committed funding output.
6. A separate payer sends the merchant's 1 CKB probe through the provider.
7. Merchant verifies delivery. The agreed capacity must remain available after the probe.
8. Merchant pays the provider directly. Core credits only a matching, confirmed CKB payment after delivery.

The prototype vault flow is archived in [the legacy guide](docs/legacy-vault-readme.md) and remains available only through explicit `LIQUIDLANE_PRODUCT_MODE=legacy_vault`. Existing deployed scripts and recovery records are preserved. Never treat legacy LP assets as marketplace provider capital.

## Documentation

- [Custody, protocol, and trust boundaries](docs/marketplace-design.md)
- [Automatic provider setup and local launch](docs/automatic-provider-setup.md)
- [Deployment, connector operation, backups, and recovery](docs/marketplace-operations.md)
- [HTTP API](docs/marketplace-api.md)
- [Testnet evidence](docs/marketplace-testnet-evidence.md)

## Verification

```sh
cargo fmt --check
cargo test --locked
bash scripts/check-rust-line-count.sh
```

Tests use isolated fixtures. Runtime requests require real signatures, node responses, and committed testnet transactions. CLI wallet commands are explicit local tools; the coordinator does not load or invoke them.

Licensed under MIT. Third-party components keep their respective licenses.
