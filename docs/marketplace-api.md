# Marketplace HTTP API

All amounts in quotes/offers are **whole CKB**, not shannons. Node evidence balances use Fiber's hexadecimal shannon fields. All times are Unix seconds. Use the exact configured frontend origin for browser requests. JSON request bodies are limited to 64 KiB.

## Accounts

| Method and path | Input | Result / authorization |
| --- | --- | --- |
| `GET /health` | — | Network, protocol, pinned Fiber version, service model |
| `POST /auth/challenge` | `address` | Single-use `challenge_id`, signed `message`, `expires_at` |
| `POST /auth/verify` | `challenge_id`, `proof` | 12-hour `token`, address, expiry |
| `POST /auth/logout` | `{}` | Revoke bearer token |
| `GET /market/dashboard` | — | Authenticated wallet's nodes, offers, orders |

Proof is either JoyID (`scheme: joyid`, `address`, `pubkey`, `signature`, `message`, `challenge`, `keyType`, `alg`) or native CKB (`scheme: ckb_secp256k1`, `address`, compressed `pubkey`, compact ECDSA `signature`). Native signatures use SHA-256 of the exact challenge string and must match the address lock. JoyID verification also checks the fixed testnet credential registry. There is no address-only login.

## Participant actions

These require `Authorization: Bearer` with an account token, except public offer discovery.

| Method and path | Input | Meaning |
| --- | --- | --- |
| `GET /market/offers` | — | Live/expired-telemetry distinction, remaining advertised capacity, fixed opening fee |
| `POST /market/offers` | `provider_node`, `min_capacity_ckb`, `max_capacity_ckb`, `opening_fee_ckb`, `public_channel` | Replace that provider's previous offer only when online and funded for its minimum; expires in 24 hours |
| `POST /market/offers/{id}/disable` | `{}` | Owner pauses new requests |
| `POST /market/nodes/pair` | `role: provider \| merchant`, `label` | Single-use pairing code valid for 10 minutes |
| `POST /market/orders` | `offer_id`, `merchant_node`, `capacity_ckb`, `idempotency_key` | Reserve capacity, await provider signature; key length 16–100 |
| `POST /market/orders/{id}/accept` | `proof` | Merchant signature over `Order::approval_message()` |
| `POST /market/orders/{id}/cancel` | `{}` | Merchant cancellation before opening is claimed |
| `POST /market/orders/{id}/verify` | `{}` | Merchant asks to verify fresh delivery and anchor its fee window |
| `POST /market/orders/{id}/fee` | `tx_hash` | Verify matching committed direct payment; retries use the same hash |
| `POST /market/orders/{id}/waive-fee` | `{}` | Provider explicitly waives an unpaid fee |
| `GET /market/orders/{id}/receipt` | — | Either participant exports the signed delivery snapshot, current order, and audit events |

Pairing creation also returns `pairing_id` (the SHA-256 identifier of the code), `label`, `role`, `account`, and `expires_at`. `GET /market/nodes/pair/{pairing_id}` requires the same account's bearer token and returns `status: waiting | expired | paired`, `expires_at`, and the exact registered `node` or null. Completion is written atomically with code consumption. This read never returns a code or node token; online/funding readiness depends on subsequent signed heartbeats.

Quote signing bytes are `LiquidLane provider quote\n` followed by JSON serialized in the exact `Quote` field order in `src/marketplace/model.rs`. `quote_hash` is hexadecimal SHA-256 of those bytes. The frontend canonicalizes that order and verifies the provider's compact secp256k1 signature before asking for merchant acceptance. The connector independently rechecks both signatures and local policy.

## Local connector protocol

Registration requires the one-time pairing code plus a node signature binding that code, pubkey, and peer address. A successful response contains a node-scoped bearer token. Subsequent node mutations require the token **and** a signed payload containing the registered `node_id` and `at` within 60 seconds; heartbeat timestamps cannot go backwards.

| Path | Node capability |
| --- | --- |
| `POST /connector/register` | Prove node control and redeem pairing code |
| `POST /connector/heartbeat` | Report funding wallet balance/address, reserve, network, version, and optional provider policy |
| `GET /connector/orders` | Retrieve orders involving this node only |
| `POST /connector/orders/{id}/quote` | Provider signs exact quoted terms |
| `POST /connector/orders/{id}/start` | Provider claims execution after both signatures and local policy authorization |
| `POST /connector/orders/{id}/failure` | Report bounded error; uncertain submitted work remains reserved |
| `POST /connector/orders/{id}/evidence` | Signed current channel observation tied to confirmed funding |
| `POST /connector/orders/{id}/probe` | Merchant advertises its invoice and later confirms `Paid` |

There is no generic RPC proxy or coordinator-issued payment/closure command. Explicit local CLI commands contact the node directly and continue to work while Core is stopped.

The heartbeat optionally includes `funding_address`, which must parse as a CKB testnet address. Authenticated node records expose `funding: { address, payload, signature }`, preserving the exact signed heartbeat for browser verification. The connector derives this from the node’s actual default funding script. An older connector omitting the address clears this metadata; the UI asks for an update instead of guessing an address. The node’s funding wallet and account fee recipient can differ.

A provider heartbeat's `provider_policy` contains `accept_public_orders`, `auto_approve`, `max_order_ckb`, `max_total_ckb`, `min_fee_ckb`, and `committed_ckb`. The fields are covered by the node signature. Public listings expose this policy and `automatic`; new orders capture the current `automatic` mode without changing quote signing bytes. Discovery and admission subtract reservations from available wallet funding and remaining policy budget, cap the result by the per-order limit, and reject fees below the current minimum. Older nodes without policy metadata remain identified as manual/restricted. Heartbeat metadata is advisory for discovery; the connector rechecks its own configuration and native state before spending.

`401` means missing/expired/wrong-scope session; `404` means a record/route is absent; `400` includes invalid signatures, policy failures, stale evidence, and unconfirmed chain evidence. A network timeout after an opening or fee submission is **uncertain**, not proof of failure. Keep the local journal/signed transaction and reconcile it.

## Settlement observations

Channel evidence retains Fiber's `state` and optional `state_flags`. The optional `settlement` object contains `closing_tx_hash`, `transaction_hashes`, `pending_outpoints`, `confirmed`, and `checked_at` (Unix seconds). Older records and connectors without these fields remain readable. `settlement_tx_hash` remains the native shutdown hash and may be absent or differ from the actual closing transaction.

The coordinator verifies submitted settlement proofs against its own CKB RPC, requires the graph to spend this order's funding outpoint, and supplies its own check timestamp. `confirmed` is a historical native-contract settlement result; it is not a wallet balance or a recovered-amount claim. A channel that settles before delivery becomes `failed` with no fee. A delivered order retains its status and fee history. See [operation and reconciliation](marketplace-operations.md#direct-closure-and-settlement).
