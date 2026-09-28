# Marketplace testnet evidence — 25–26 September 2026

**A real opening, routed payment, direct fee, coordinator outage, and cooperative settlement completed on CKB testnet.** All three nodes were operated by this project on one machine with separate wallets, keys, and stores. This demonstrates the implementation; it is not an independent-provider pilot or proof of merchant demand.

**26 September update:** a separate [automatic public-order run](evidence/2026-09-26-automatic/README.md) delivered 500 CKB with no merchant allowlist or provider approval command, then completed the routed probe, 61 CKB fee, and cooperative recovery. A real user also completed JoyID acceptance and fee payment as recorded in the [runbook](marketplace-operations.md#validation-limits). The original run and its validation limits are retained below as historical evidence.

## What ran

- Official Fiber **0.9.0**, source commit `e6cb7ac7770b1798a1ad5dfb9a8f4ae5db52036f`; archive SHA-256 `4085453de9a3f7ca0f0aeb7db9e34c0af4d34feb84566be554a5c70557cecbea`.
- CKB testnet, native CKB, two public bidirectional channels: payer → provider → merchant.
- Marketplace SQLite database and real wallet/node signatures. Native local-wallet signing exercised account authentication, quote acceptance, and the fee. Browser JoyID signing remains a separate validation gate.
- Order `968e1e0cfe39f30a2c0cba3f9df28df5b92078a3c1ed847d136880fb9fae3710`.
- Quote: **500 CKB usable inbound after the probe**, 600 CKB provider contribution, 99 CKB merchant reserve, **61 CKB direct opening fee**. The fee is a testnet cell-creation constraint, not validated commercial pricing.

## Recorded results

| Check | Actual result / evidence |
| --- | --- |
| Signed order and local approval | Provider quote, merchant acceptance, local provider approval, and audit events in [the signed receipt](evidence/2026-09-25/final-receipt.json) |
| Merchant channel funding | [Committed funding transaction](https://pudge.explorer.nervos.org/transaction/0x16e05c95a87d4da883f34234eb6ea07d7f89141bbcb8b5ce5dbc3195e5f6c33b), output 0 |
| Incoming route funding | [Committed payer/provider funding](https://pudge.explorer.nervos.org/transaction/0x840c21bbd998f05802949098e2c0109e0c302a4e46d569c7c7662fd9254f6816), output 0 |
| Delivery | Separate payer sent 1 CKB through the provider; merchant invoice `Paid`; 500 CKB inbound remained. [Payer result](evidence/2026-09-25/routed-probe-payment.json), [merchant invoice](evidence/2026-09-25/merchant-probe-invoice.json) |
| Timing | Created 18:09:28 UTC; delivery verified 18:18:50 UTC: **562 seconds**, including manual setup/debugging. One run, not a latency benchmark. |
| Opening fee | [61 CKB fee transaction](https://pudge.explorer.nervos.org/transaction/0xe5e88a90687ad40843a926e70cf85bde8a1950242654745af3ebc19d314a6735), committed after delivery and credited once |
| Restart | Core/connectors reconciled the existing channel. After the final restart the order remained `delivered` / `paid`, with `Closed` channel observations and zero current inbound. |
| Coordinator offline | Core stopped; a second 1 CKB payment succeeded on the same two-channel route. [Native payer and merchant results](evidence/2026-09-25/outage-proof.json) |
| Closure without Core | Provider submitted native cooperative closes for both channels; both closing transactions committed. See the balances below. |
| Peer offline | Each side submitted a forced close while the other node was stopped. Both commitments and subsequent settlement transactions confirmed; participant wallet recovery was verified on 26 September. Native RPC states remained stale. |
| Backup | Consistent SQLite backup restored for integrity/record inspection; paid order preserved, file mode 0600. [Result](evidence/2026-09-25/backup-verification.json) |

The first probe attempt failed because the payer's route channel was not ready. Its native payment state was checked before retrying; a later attempt succeeded. No payment success was inferred from submission alone.

## Balance reconciliation

After two 1 CKB payments, the provider held 499 CKB spendable balance in the merchant channel and 2.002 CKB in its incoming channel. It earned 0.001 CKB routing fee per payment. The merchant held 2 CKB earned balance. Native reserves are additional to these channel balances.

| Closing transaction | Provider output | Other participant output |
| --- | ---: | ---: |
| [Provider/merchant](https://pudge.explorer.nervos.org/transaction/0xf893a82769c35e36dc3a42bc5f66d0703b9a7ad788c383253dbf289782ee94f2) | 597.99998944 CKB | Merchant: 101 CKB |
| [Payer/provider](https://pudge.explorer.nervos.org/transaction/0xf483cb650b58b115e09b6d4d8eb3921f5b6fbe72125909e664a16c238155ae65) | 101.00198944 CKB | Payer: 997.998 CKB |

Provider channel returns total **699.00197888 CKB**: 699 contributed + 0.002 routed fees − 0.00002112 closing fees. The direct 61 CKB opening fee is separate. Merchant output is its 99 CKB reserve plus 2 CKB earnings. These are actual closing outputs; subsequent test spending may consume them. [Committed transactions](evidence/2026-09-25/cooperative-settlements.json) and before/after node snapshots accompany this record.

## Peer-offline recovery boundary

Two additional private, bidirectional recovery channels were funded separately from the completed purchase:

| Case | Channel | Confirmed commitment |
| --- | --- | --- |
| Merchant offline; provider force closes | `0xce665b51cc0ab3c4836879f4d47e71c1b9e727baf280e071b7397a8cf9cd1065` | [Commitment](https://pudge.explorer.nervos.org/transaction/0x48ec70fdcc1732b798d576284f9da107ef159ae95285d209aadfddd07fd5ed4a) |
| Provider offline; merchant force closes | `0xa5be6b489215afcf198aad0e15b3a2744115fb3debbcc1efcc72f83bc7c19bd7` | [Commitment](https://pudge.explorer.nervos.org/transaction/0xc815ffa86948ba2c9dc6ff268cbe5f18d335eb8065fd158db151840803bcb2a3) |

Both nodes were restored from their current stores after the outage tests. Each commitment initially held **298.99999544 CKB** under the native commitment lock. [Initial chain observations](evidence/2026-09-25/forced-commitments.json) recorded live commitment outputs while native timelocked settlement was pending. Fiber 0.9.0 requires at least one epoch of commitment delay.

**Follow-up on 26 September:** both commitments were spent through native settlement stages, producing confirmed participant wallet outputs. The [complete transaction trace](evidence/2026-09-25/forced-settlements.json) resolves every contract output and deducts additional wallet inputs used by the watchtowers. Across the two cases, each participant contributed 299 CKB and recovered **298.99998122 CKB net**. Combined commitment and settlement fees were **0.00003756 CKB**. Gross wallet change outputs are larger because native settlement also consumes existing wallet cells; they must not be counted as new recovered principal.

| Case | First settlement stage | Final contract-output spend |
| --- | --- | --- |
| Merchant offline | [Counterparty settlement](https://pudge.explorer.nervos.org/transaction/0x6388f05af2ead4712695ff63ab996d655adcfd6e34b7190459a802fe5b8a169e) | [Final provider recovery](https://pudge.explorer.nervos.org/transaction/0x9283e5ef4b03a55bcb30517a6cf2d2250d17b897ea7c4b4d9d7298890fba8943) |
| Provider offline | [Counterparty settlement](https://pudge.explorer.nervos.org/transaction/0xf9763fed056e5e47a3d64047445a57a46f381ccf759ff09eefb58ee343d433c3) | [Final merchant recovery](https://pudge.explorer.nervos.org/transaction/0xb48f813b002960cbaea25f94786089fc539edfa4356ac6d365b3d0b717d586e6) |

The provider's first channel still reported `WAITING_COMMITMENT_CONFIRMATION`; the other native observations retained `WAITING_ONCHAIN_SETTLEMENT` despite completed chain recovery. [Recorded native states](evidence/2026-09-25/recovery-followup.json) document this discrepancy. Chain evidence establishes recovered funds; native RPC state/restart reconciliation remains an integration issue before a public pilot.

The read-only `scripts/inspect-testnet-recovery.py` reproduces this trace from the initial commitment records and participant `node_info` responses. It checks testnet genesis, committed spending transactions, intermediate contract outputs, wallet locks, and net wallet changes without signing anything.

The deployed CLI was corrected during this test: v0.9.0 forced `shutdown_channel` requires omitting cooperative `close_script` and `fee_rate` fields. A rejected RPC call did not count as closure.

## Reproduce and inspect

Follow [the deployment/runbook](marketplace-operations.md) with independently funded testnet nodes. Pair provider and merchant; publish an offer; request and sign its quote; approve locally; fund a payer route; pay the actual merchant probe; verify delivery; transfer and confirm the direct fee. Record balances, stop only the coordinator, then repeat a payment and close locally. Test peer outages on separate channels with current state and native monitoring.

For this workstation, the coordinator uses `127.0.0.1:18180`, the app uses `localhost:3100`, and provider/merchant/payer RPCs use loopback ports 18227/18327/18427. Private configs, encrypted node keys, local test-wallet keys, journals, and backups are under ignored `runtime/testnet`. They are not included in this evidence directory.

The artifact [manifest](evidence/2026-09-25/manifest.json) hashes the public evidence files. Payment preimages are omitted; real payment hashes/statuses and public transaction witnesses remain. Screenshots show the actual provider offer: [desktop](evidence/2026-09-25/desktop.png), [mobile](evidence/2026-09-25/mobile.png).

## Release checks and remaining gates

- **61 Rust tests** passed, including 12 marketplace/connector tests and 49 retained legacy tests; formatting and the 300-line Rust source limit passed.
- Frontend lint, production compilation/type checking, and four desktop/mobile live-API and unavailable-service browser checks plus the JoyID transfer-unit contract check passed.
- Runtime uses actual signatures, node RPCs, invoices, and confirmed CKB transactions. Test fixtures are confined to regression tests.
- Still required: live JoyID browser login/acceptance/fee signing, native RPC recovery-state reconciliation, independent operator/merchant trials, market/pricing evidence, and public hosting validation. On-chain recovery of both forced-close cases is complete.
- The frontend Noble library independently verified the actual Rust-signed quote. JoyID's live public credential endpoint returned its expected schema. Fee requests use shannons, matching the [official SDK example](https://github.com/nervina-labs/joyid-sdk-js/blob/main/examples/ckb-demo/src/pages/SignTransaction/index.tsx).
- The real JoyID popup opened from the compiled app. Its account-creation flow rejected the automated Linux/Chrome 151 authenticator environment; this attempt did not complete wallet authentication or signing.
- Legacy vault records remain preserved and inventoried; historical positions require their own on-chain reconciliation. This marketplace does not resolve or consume them.
