# Provider funding

The provider workspace now separates connection, capital, offer publication, and merchant requests. Pairing and publishing do not claim to deposit money. The funding address comes from the actual Fiber funding script, is signed by the paired node, and is verified by the browser before a new transfer.

A real **61 CKB** transfer from the owned payer test wallet reached the existing provider node:

- Transaction: `0x5250ca7f06944c2ef1f3cb899d19df060d114696ed687aaebea9215767ef74c0`
- CKB status: committed; block `0x15806c6`.
- Node wallet capital: **1,483 → 1,544 whole CKB**, as reported by the real connector.
- Browser: confirmed the saved transaction, displayed the new balance, published a 600 CKB minimum offer, and preserved the receipt before enabling another top-up.
- An isolated real coordinator was used for browser publication. The running marketplace's existing user order remains delivered and paid; no new channel was opened by this funding check.

`capital-transaction.json` contains the real signed transfer. `summary.json` contains the participants, confirmed hash, balance observations, and exact signed node report. `confirmed-capital.png` shows the result. `desktop.png` and `mobile.png` show the preceding real-node onboarding checks. `sha256.json` records artifact hashes.

The test used the native local signer with an explicitly provided owned test wallet. It exercised the browser's actual transaction confirmation and receipt recovery against CKB, not JoyID passkey signing. JoyID capital signing still requires the user's manual browser retest. The previously completed JoyID opening-fee transaction is separate evidence.

Checks recorded with this run: 73 Rust tests, 19 wallet/payment tests, six desktop/mobile tests against real APIs and Fiber nodes, and one real capital-flow test passed. Production build, TypeScript, lint, Rust format, and source-size checks passed. Failure tests cover modified funding addresses, incorrect node/network/owner, stale reports, unexpected outputs, uncertain submissions, and late confirmation overwriting newer payment records. Offer creation rejects offline nodes and insufficient capital including existing reservations and the reserve/change buffer.

The live capital test is opt-in (`LIQUIDLANE_FUNDING_TEST=1`, an owned `LIQUIDLANE_FUNDING_WALLET_KEY`, both live connector configs, and an evidence directory). Its first run sends 61 testnet CKB. If the directory already holds this transaction and its matching summary, rerunning reconciles that same payment without sending again. Do not remove its receipt to retry a failed observation.
