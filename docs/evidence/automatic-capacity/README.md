# Automatic capacity delivery

An actual CKB testnet order delivered 500 CKB of initial receive capacity with an empty merchant allowlist and automatic provider approval. The merchant signed the quote; no provider `approve` command was issued. The local opening journal records `approved: false`, `attempted: true`, and a 600 CKB funding commitment. Core recorded `automatic_provider_authorization` before native funding.

| Evidence | Result |
| --- | --- |
| Order | `ea21ff1e6681b8fe2230426b52a1cd35d9de0316023ea4d305522997a3c8b506` |
| Provider policy | Any merchant; automatic; 2,000 CKB per order; 5,000 CKB total; 61 CKB minimum fee |
| Channel | `0x7f880f5d5846fad277901c1306ed227b22a873125bc03e871baba4f5fa175069` |
| Native funding | `0x3c31462b883f62e20393b41bb868a9320cfa5b236d160f21df05d24f2bef1502#0` |
| Routed probe | 1 CKB; payer `Success`, merchant `Paid`; 500 CKB inbound remains |
| Opening fee | 61 CKB, confirmed as `0x5cb5836713358476da1a1d4d24c9b82a5a38429968cdfae8b9b4c5e0cf99f260` |
| Test cleanup | Cooperative closing transaction `0x65a91b4b6f3c3d31c8d122a9c26310df76faa5130e9e9710b090798198f4bc0d` committed; both native nodes report `Closed` |
| Returned channel funds | Provider: 599 CKB; merchant: 99.99998944 CKB, including the received probe and after the closing fee |

The provider's keys and funding stayed on its node. The payer used the existing payer-to-provider route. This test's channel was closed after fee settlement and was separate from the completed JoyID order.

[summary.json](summary.json) contains the actual policy, selected local journal fields, payer result, and closure observations. [receipt.json](receipt.json) contains the provider quote, native merchant acceptance, signed delivery observations, and coordinator events. The [fee transaction](fee-transaction.json) and [closing transaction](closing-transaction.json) are public signed CKB transactions. [sha256.json](sha256.json) records the artifact hashes. No node keys, session tokens, or registration credentials are included.

Checks recorded with this run: 69 Rust tests, formatting and source-size checks, frontend lint and production build, four desktop/mobile public-API checks, and two desktop/mobile onboarding checks using real Fiber nodes. The onboarding tests verify an automatic pairing file without merchant addresses and hide the manual approval command for automatic orders.

All nodes in this run are team-operated. The automatic opening test uses native wallet signatures; it does not automate a JoyID passkey. The probe, delivery verification, fee payment, and cleanup were explicit test actions. Only quote signing and channel connection/funding are automatic in the product. Fees remain payable after verified delivery and are not guaranteed to be collected. Independent operator trials, public hosting, and guaranteed-duration enforcement remain separate work.
