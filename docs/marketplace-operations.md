# Testnet deployment and operation

## Installation

Build with Rust 1.91 using the committed Cargo lockfile. The app uses Node 22 and its committed npm lockfile. Use Fiber **v0.9.0** on every participant node. Do not upgrade one participant independently of the protocol pin.

The tested official Linux portable release is:

`https://github.com/nervosnetwork/fiber/releases/download/v0.9.0/fnn_v0.9.0-x86_64-linux-portable.tar.gz`

Published SHA-256: `4085453de9a3f7ca0f0aeb7db9e34c0af4d34feb84566be554a5c70557cecbea`. Verify the archive before extracting/running it. Native testnet script configuration comes from that release. Preserve node keys and stores when updating a binary; never substitute an empty node directory for a funded node.

## Coordinator

```sh
cp .env.example .env.marketplace
cargo build --release --locked --bins
./target/release/liquidlane-core
```

This runs the working local configuration at `127.0.0.1:18080` for frontend origin `http://localhost:3000`. The coordinator checks the CKB genesis at startup. It does **not** load the historical `.env` file in marketplace mode.

For hosting, supply the actual HTTPS frontend origin in `LIQUIDLANE_MARKET_ORIGIN`, `LIQUIDLANE_ENV=production`, a writable persistent `LIQUIDLANE_MARKET_DB`, the desired bind address, and a working CKB testnet RPC/indexer. The Dockerfile and Railway entrypoint default to marketplace mode and `/data/liquidlane-marketplace.sqlite3`. A reverse proxy terminates TLS; `/health` is the health-check path. The container runs as UID 10001; give that UID access to its data volume.

Use a **single coordinator process** with its persistent SQLite database for this pilot. Put request-rate and connection limits at the public reverse proxy; apply tighter limits to `/auth/challenge` and `/connector/register`. Do not expose participant Fiber RPC ports, keys, node stores, backup files, or operator token files through that proxy.

The coordinator needs no deployment key, vault signer, Fiber RPC password, or node identity key. Provision only its marketplace environment variables. CORS is an origin restriction, not authentication; every nonpublic action has server-side authorization.

## Participant connector

For a new provider node, use [automatic provider setup](automatic-provider-setup.md): one local command installs Fiber and starts both background services, then the browser guides funding. The following steps cover an existing node.

1. Run a CKB testnet Fiber 0.9.0 node; keep RPC on loopback. Permit P2P access only as needed for actual peers.
2. Choose **Request Capacity → My receiving nodes** to receive payments as a merchant, or **Supply Liquidity → My offers & nodes** to fund channels as a provider. The workspace fixes the node's role. The node label is a display name you choose; the suggested name works. Click **Create pairing code**, then **Download pairing file**. Only providers enter funding limits and a minimum fee. New provider setup accepts public merchant requests; no merchant wallet list is required.
3. Put `liquidlane-pairing.json` in a separate connector folder on the machine running that node. Install the connector from the Core checkout, then run setup from the folder containing the download.

```sh
cargo install --locked --path . --bin liquidlane-connector
liquidlane-connector setup ./liquidlane-pairing.json
```

Setup asks for the local RPC URL and Fiber data directory from your running node's configuration. The directory must contain `fiber/sk`. It reads `node_info`, checks testnet and version 0.9.0, and verifies that the local identity key matches the RPC node. It detects a TCP peer address; confirm it or supply the node's actual reachable address when the provider runs elsewhere. A loopback peer address works only between nodes on the same machine.

Setup writes a private `connector.json`, pairs the existing node, and runs its connector. Add `--background` on Linux with a working systemd user session to enable automatic connector startup; keep the existing Fiber node service running separately. Keys stay on the node machine. Pairing itself moves no funds; once running, an automatic provider processes accepted orders. Keep that process running; after restarting the machine, run `liquidlane-connector run ./connector.json` from the same folder. Preserve the configuration and `connector-state` directory. Setup refuses to overwrite another node's configuration and preserves an existing matching connector's state. `--once` performs a single heartbeat/work cycle instead of running continuously.

The website tracks the exact pairing code through an authenticated status endpoint. Downloading a file alone never marks a node paired or online. Codes are single-use and expire in 10 minutes; expired setup shows a button to create a replacement. Setup progress and the selected workspace tab survive a page refresh. Existing paired nodes show actual connector status, available node-wallet CKB, and the channel reserve. A receiving node needs its reported reserve plus a 63 CKB funding/change buffer; the browser wallet separately pays the opening fee. An online, sufficiently funded receiving node shows **Find capacity**.

New provider setup writes `accept_public_orders: true`, `auto_approve: true`, and an empty `allowed_merchants` list. No customer addresses are required: any merchant may choose the published offer, and accepted requests open automatically within the local funding limits. Fund the Fiber node wallet with the capital you intend to operate; the numeric limits do not deposit or transfer CKB. Match the published fee to the local minimum. A quote's provider funding includes capacity, reserve, and probe headroom, so the maximum advertised capacity must fit under `max_order_ckb`. Signed heartbeats carry these limits and current channel commitments to discovery and reservation checks. Wallet availability excludes occupied contract/token cells and already-funded channels.

Existing configurations without `accept_public_orders` remain restricted; an empty list still authorizes no customer in that legacy mode. To enable marketplace automation, install the updated connector, stop its current process, preserve its config/state, set `accept_public_orders` and `auto_approve` to `true`, check its funding limits, and restart it. Alternatively, use **Set up automatic requests** in the node card and run the downloaded setup in the same connector folder. Restricted/manual operators can still approve locally:

```sh
liquidlane-connector status ./connector.json
liquidlane-connector approve ./connector.json "$ORDER_ID"
```

The connector independently verifies the accepted quote and signatures, checks funds, connects to the exact quoted peer, claims the order, persists its attempt, and requests a bidirectional opening with its own closing lock. It then reconciles temporary/final channel IDs and confirmed funding. Multiple plausible channels stop reconciliation for inspection.

Tokens expire after 90 days. Re-pair with the same account/node to rotate and invalidate the previous token. Keep the connector state directory private, with registration credentials and SQLite journal intact. A missing registration token may be recovered by pairing; a lost opening journal must be reconciled against the actual node before any new approval.

## Provider capital and the connector

The provider workspace has four steps: **Connect node → Add CKB → Publish offer → Serve merchants**. Downloading a pairing file alone creates neither a node nor a capital deposit. “Action needed · Run connector” means local setup has not yet paired a live node. Automatic setup with `--new-node --background` creates an unfunded node and runs Fiber and its connector as services. On the fresh background-service heartbeat, the UI moves to **Add capital**; manual setup retains **Continue to add capital**. The provider still operates the node machine, and signs the capital transfer in their browser wallet.

**Fund your node** shows its available wallet CKB and its signed funding address, derived from Fiber’s `default_funding_lock_script`, not its peer identity. Setup prints that address locally for comparison. The browser verifies the report signature, node ID, testnet genesis, version, owner, and freshness before a new transfer. Existing connectors need the updated binary and a restart to report this address.

The capital button asks JoyID to sign a real transfer to that node’s wallet. The app validates exact recipient/amount and change outputs, normalizes CKB dependency enums, dry-runs the signed transaction, and stores its bytes and hash before broadcasting. A timeout resumes the same signed transaction. Confirmation permits a new top-up, preserving the earlier receipt locally. Multiple tabs serialize new payments for the same node. Capital availability comes from the node’s actual CKB balance report, never a downloaded file or unconfirmed receipt. If the connected wallet is already the node’s wallet, existing CKB is recognized directly; send additional CKB from another wallet.

For one 500 CKB offer with a 99 CKB reserve, the node needs **663 CKB** free: 500 capacity + 99 reserve + 1 probe headroom + 63 CKB change/fee buffer. Existing request reservations add to this requirement. Published capacity is also bounded by the node’s per-order and remaining total funding limits. The API refuses a new offer if the node is offline or cannot fund its minimum. This check does not fund a channel; accepting a signed quote triggers the connector’s local checks and native channel opening.

LiquidLane discovers providers, obtains signed quotes, coordinates openings, and records evidence and fees. Providers keep their channel keys. Capital in channels is not a platform balance that can be withdrawn from the website; native closure and confirmed settlement return available capital under Fiber’s rules.

## Delivery and fee

The browser shows **Quote → Accept → Open channel → Test payment → Verify delivery → Pay fee**, with the responsible participant and next action for each state. A quote request makes no payment. The merchant reviews and signs the provider's quote; an automatic provider then funds the channel without an operator command. Listings and requests identify automatic versus legacy manual operation. Duplicate requests point back to the existing order. Expired or cancelled orders can be replaced before funding; uncertain openings require reconciliation. Pausing an offer stops new requests; already accepted quotes can still open.

The merchant connector creates a real 1 CKB invoice only after observing the channel. A separate payer uses its normal Fiber wallet or the explicit local probe command:

```sh
liquidlane-connector pay-invoice ./payer-connector.json "$INVOICE"
liquidlane-connector payment-status ./payer-connector.json "$PAYMENT_HASH"
```

This CLI restricts probe payments to 1 CKB testnet invoices and a 0.01 CKB fee cap. It records attempts before sending. It retries only when the native payment session reports `Failed`; Fiber also rejects retries with attempts still in flight. The coordinator never invokes this command.

After merchant verification, pay the agreed fee from the authenticated wallet to the signed recipient. The browser saves exact signed transaction bytes before broadcasting. A timeout is not a rejection: inspect the saved hash or resubmit those same bytes. Fee credit requires a committed transaction in a block after the delivery anchor. Export the signed receipt before deleting browser or connector state.

The optional `liquidlane-wallet` binary supports native CKB test wallets for reproducible integration. `address`, `login`, `approve`, and `transfer` read an explicitly supplied local raw key file. `transfer` writes the signed transaction/known hash before submission and refuses to overwrite an existing receipt. It is not a coordinator service and does not decrypt Fiber's encrypted CKB wallet file.

## Backup and crash recovery

```sh
python3 scripts/backup-marketplace.py ./liquidlane-marketplace.sqlite3 ./backups/marketplace.sqlite3
```

Use a new destination for each backup. The SQLite backup API includes committed WAL state and checks integrity. Back up connector journals similarly, and protect registration files separately. Keep backups off the served web path and outside the container's ephemeral layer. Test restoration to a separate service/database before replacing a running instance.

For interrupted openings:

- Read the order, connector journal, node `list_channels`, funding outpoint, and CKB transaction status.
- `attempted=true` prevents a second opening. An RPC timeout retains the reservation and enters reconciliation.
- A crash after Core claims execution but before the local journal is durable can leave an opening with no attempt. Inspect the real node; do not reset it to accepted automatically.
- Unconfirmed/unknown funding is not failure and does not unlock reservations. Manually resolve ambiguous cases with the node operator and preserve the audit trail.

Back up Fiber's current store and keys using its native procedures. The marketplace database is not a channel backup. Keep native watchtower/monitoring available during counterpart outages. Do not force close from an old restored snapshot.

## Direct closure and settlement

Participant-initiated commands work without the coordinator:

```sh
liquidlane-connector close-channel ./connector.json "$CHANNEL_ID"
liquidlane-connector close-channel ./connector.json "$CHANNEL_ID" --force
```

The cooperative command uses the node's default closing script. The forced command uses the native previously negotiated policy; it cannot choose another destination. Only use force closure after inspecting current node state and outstanding TLCs. The commands are explicit local operations, never automatic responses to stale marketplace telemetry.

Check the closing transaction on CKB and inspect the actual output locks, capacities, and live/spent state. Cooperative wallet outputs can become spendable after confirmation. Unilateral commitment outputs can remain timelocked even if Fiber reports `Closed`; v0.9.0 enforces a minimum one-epoch commitment delay. Record that as pending recovery until native settlement creates confirmed spendable wallet outputs. Reconcile all provider channels, including incoming routed value; merchant earnings remain merchant-owned.

The recovery checks also observed stale native states after funds had reached wallet outputs: `WAITING_COMMITMENT_CONFIRMATION` or `WAITING_ONCHAIN_SETTLEMENT` remained in RPC responses. LiquidLane now reconciles these independently against CKB and displays the native observation alongside confirmed settlement. It does not rewrite Fiber's store or clear its flags. Do not reset a channel, reopen an order, or resubmit force closure merely to clear those labels.

The connector follows the actual spend of the funding outpoint, including intermediate native commitment outputs. It does not assume the native shutdown hash is the transaction that was broadcast. A complete proof requires committed transactions resolving every descendant contract output to standard native CKB wallet outputs. Wallet arrivals that have subsequently been spent still count as historical settlement; their gross output capacities are not presented as recovered principal or current spendable balance. Unknown scripts, typed outputs, missing indexer records, RPC errors, and scan limits retain the reservation. This testnet verifier supports the pinned Fiber 0.9.0 testnet contracts and standard secp256k1 closing wallets.

Closing channels continue to count against the provider's total funding limit until that proof succeeds, including channels that Fiber already calls `Closed`. Confirmed settlement frees that limit even if Fiber still calls the channel `ShuttingDown`. Local checks are cached for at most 60 seconds, revalidated after expiry, and never renewed from an old proof during an RPC failure. The coordinator independently fetches and checks every transaction in a submitted proof. Closure before verified delivery ends the order as failed with no fee; previously delivered orders keep their delivery and fee history. Delivery verification also requires the funding cell still to be live.

Inspect an individual channel without a running coordinator, without changing Fiber, and without sending a transaction:

```sh
liquidlane-connector settlement-status ./connector.json '<funding-transaction-hash>#0'
```

The result is `null` when no committed funding spend can be established, a pending proof when outputs remain unresolved, or `confirmed: true` with the complete transaction list. Current wallet availability is still measured separately from live wallet cells. See the [settlement and service validation](evidence/settlement-recovery/README.md) for the two historical forced closures checked through this production verifier.

For the recorded peer-outage cases, this read-only inspector follows intermediate settlement outputs and deducts existing wallet cells used to pay fees:

```sh
python3 scripts/inspect-testnet-recovery.py runtime/testnet/forced-commitments.json runtime/testnet/provider-node.json runtime/testnet/merchant-node.json runtime/testnet/forced-settlements.json
```

For another deployment, supply its actual commitment transaction records and both public `node_info` responses. Confirmed wallet arrivals can later be spent; inspect their recorded live/spent status and net changes instead of adding gross change outputs to recovered principal.

## Legacy boundary

The old API is available only with explicit process environment `LIQUIDLANE_PRODUCT_MODE=legacy_vault`; its original `.env`, JSON state, builders, deployed scripts, and archived UI are retained. It must be operated separately for existing obligations. The new marketplace never migrates deposits into provider balances.

Implementation preserved a private snapshot under `runtime/legacy-backup`. Its local ledger inventory contains 13 active LP positions, 17 reserved and 1 deployed capacity reservations, and 1 channel marked active. These are historical records, **not a current on-chain reconciliation**. Resolve them through the legacy recovery path before retiring that deployment. The original ledger checksum is recorded in the private inventory.

## Validation limits

The regression suite covers real cryptographic verification and failure/replay invariants. The testnet evidence records native-wallet signing, actual funding, routed payments, direct fees, coordinator restart/outage, and closure. The [automatic capacity run](evidence/automatic-capacity/README.md) additionally verifies an empty merchant list, automatic opening without a provider approval command, a paid probe, the direct fee, and cooperative recovery.

The initial automated JoyID account-creation attempt with Chrome's virtual authenticator returned an unsupported-environment message. A real user subsequently completed JoyID login, quote acceptance, and the 61 CKB fee. Order `0d9f18529e80f74408c0ed2b9b0c8ebb0e5272a90543602b6199299d03baeba8` has a verified JoyID acceptance and paid fee transaction `0x53d73ef6701356a325614416e837514fb17c97b40d66f3260cec0d9766d43a6e`. This is one real browser environment, not broad wallet compatibility coverage or an automated passkey test.

The [provider-capital check](evidence/provider-funding/README.md) confirms a real 61 CKB top-up, signed node balance update, funded offer publication, and receipt retention. This uses an owned native signer; JoyID capital signing remains a manual browser check.

The guided-setup browser regression uses an isolated real coordinator and existing live Fiber nodes. Native-wallet signatures authenticate dedicated test accounts. It exercises automatic provider setup without merchant addresses, rejects a mismatched node directory, verifies pairing/heartbeat/funding readiness, requests a real provider-signed quote, and tests automatic-opening messaging. It cancels before another provider cycle, so this UI test never funds a channel. It does not substitute for a user's JoyID passkey or fee-signing test.

Next.js was upgraded to 16.3.6 after checking the upstream security advisory. npm audit reports no high/critical vulnerabilities; three low-severity entries remain through the JoyID/CKB SDK's `elliptic` dependency. The recommended audit downgrade would break the wallet SDK and was not applied. Marketplace quote verification uses Noble; server verification uses Rust cryptographic libraries.

Independent operators, merchant demand validation, guaranteed-duration enforcement, and a production security review remain separate from this team-operated testnet demonstration. Testnet fees are not real revenue.
