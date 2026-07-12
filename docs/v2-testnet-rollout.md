# LiquidLane V2 Testnet Rollout

This is the checklist for turning `vault_external` from safe-blocked beta mode into real vault-funded Fiber execution on CKB testnet.

## Required Env

Core must stay in product mode:

```env
LIQUIDLANE_FIBER_FUNDING_MODE=vault_external
LIQUIDLANE_VAULT_SCRIPT_VERSION=v2
LIQUIDLANE_VAULT_FUNDING_BUILDER_ENABLED=true
LIQUIDLANE_VAULT_FUNDING_SIGNER_ENABLED=true
```

The active v2 script config must include:

```env
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0x...
LIQUIDLANE_VAULT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x...
LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x...
LIQUIDLANE_REQUEST_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH=0x...
LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1...
LIQUIDLANE_VAULT_CELL_OUT_POINT=0x...#0x0
```

## Current Guardrail

If any v2 value is missing, Core reports `external_funding_ready=false` and leaves merchant requests at `funding_required`. This is intentional: reserved LP liquidity must not be represented as usable Fiber capacity until a verified vault-funded CKB funding transaction exists.



## Current Testnet Deployment Values

The July 12 testnet deployment broadcast the full six-script v2 package and initialized a fresh active vault. The prior July 12 funding-intent-only extension remains a historical record, but Core should use the full deployment below.

```env
LIQUIDLANE_VAULT_SCRIPT_VERSION=v2
LIQUIDLANE_VAULT_LOCK_CODE_HASH=0x8e53a220c6346f1d6d390c08c5f54ff73a640e940f886a20c6ddc26618a74357
LIQUIDLANE_VAULT_LOCK_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x0
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0xcd435dde45afb390499712de339f4d13f81a5d61186a065136570d4256c54ca1
LIQUIDLANE_VAULT_TYPE_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x1
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x6136759bd57001401d19369a4de524f9a19b8839e97a0fea107e308fe98b7b29
LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x2
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x2ecd2ec19a0be5707dbf61cd8bd758d84735beb0520a64addd9fe6a89772226d
LIQUIDLANE_REQUEST_TYPE_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x3
LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH=0x4c5ecdd444594253667e19b9a473e712b5c77973159d94620cd5c2a86f3d3c45
LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x4
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=0xb3c57c3ec41803ea125250ffc51f30873e60fa827ef55b382b0c36ee9fcd4240
LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT=0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4#0x5
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1qz898g3qcc6x78td8yxq3304flmn5eqwjs8cs63qcmwuyesc5ap4wqjkg67zcsv3rnhkmqqpfp7p8x7hglmkq6hfm56s4gma8zvlfp9xdn5cxdrxqtjxx286nkllj32q5d7tfnxxxss2lywljzy4v4fac86vxcfkwkda2uqpgqw3jd56fhjjf7dpnwyrn6t6pl4pql3s3l5ck7ef9mxjasv6p0jhqldlv8xch46cmprnt04s2g9xftwanln239mjyfkm83tu8mzpsql2zff9pl79rucgw0nql2p8aa2m8q4scdhwnlx5ysqt5j8y6
LIQUIDLANE_VAULT_CELL_OUT_POINT=0x05bfc0fa84b5c8e7d3e42d312dc30c1adb676f1e3e8fa79b819720ec2aecd602#0x0
```

Explorer links:

- Full v2 script deployment: https://pudge.explorer.nervos.org/transaction/0xa328147c40b9efa8102b2e4675fc484f86219043d488fc7db960a5a18f27e7e4
- Fresh active vault: https://pudge.explorer.nervos.org/transaction/0x05bfc0fa84b5c8e7d3e42d312dc30c1adb676f1e3e8fa79b819720ec2aecd602

## Deployment Steps

1. Build RISC-V scripts with `scripts/build-ckb-scripts.sh`.
2. Deploy the v2 scripts to Pudge testnet.
3. Record explorer links, out-points, and code hashes.
4. Deploy or migrate a v2 vault cell.
5. Update Render/Core env with the v2 values above.
6. Confirm `/health` shows `external_funding_ready=true`.
7. Reserve merchant capacity.
8. Build, sign, dry-run, and submit the vault-funded CKB funding tx.
9. Submit the signed funding tx to Fiber.
10. Let the watcher mark the request `channel_open` only after Fiber reports an active channel.
