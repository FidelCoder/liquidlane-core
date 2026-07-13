# LiquidLane V2 Testnet Rollout

This checklist keeps `vault_external` in the real product path: LP vault liquidity funds merchant Fiber capacity, while the managed Fiber node only signs executor inputs and pays small cell/fee overhead.

## Required Core Env

```env
LIQUIDLANE_FIBER_FUNDING_MODE=vault_external
LIQUIDLANE_VAULT_SCRIPT_VERSION=v2
LIQUIDLANE_VAULT_FUNDING_BUILDER_ENABLED=true
LIQUIDLANE_VAULT_FUNDING_SIGNER_ENABLED=true
LIQUIDLANE_EXECUTOR_ENABLED=true
LIQUIDLANE_EXECUTOR_CKB_ADDRESS=ckt1...
FIBER_RPC_URL=https://<fiber-rpc-host>
LIQUIDLANE_CKB_RPC_URL=https://testnet.ckb.dev/rpc
```

## Required Fiber Node Env

The Fiber service must call Core's builder whenever it needs a channel funding tx:

```env
LIQUIDLANE_CORE_FUNDING_BUILDER_URL=https://<core-host>/internal/fiber/funding-builder
```

The Render Fiber entrypoint converts this into:

```env
FIBER_FUNDING_TX_SHELL_BUILDER=curl -fsS -H content-type:application/json --data-binary @- https://<core-host>/internal/fiber/funding-builder
```

## Required Script Env

```env
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0x...
LIQUIDLANE_VAULT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x...
LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x...
LIQUIDLANE_REQUEST_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH=0x...
LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=0x...
LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT=0x...#0x...
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1...
LIQUIDLANE_VAULT_CELL_OUT_POINT=0x...#0x0
```

## Current Testnet Deployment Values

The July 13 testnet deployment is the active package for the vault-funded Fiber builder path. It replaces the prior July 12 script out-points.

```env
LIQUIDLANE_VAULT_SCRIPT_VERSION=v2
LIQUIDLANE_VAULT_LOCK_CODE_HASH=0x3f77bc70751e5b7f37863fa98d1f9b217d6f45f6a8ccbf0d58a66937632dcb85
LIQUIDLANE_VAULT_LOCK_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x0
LIQUIDLANE_VAULT_TYPE_CODE_HASH=0x66a919468a594777a1a833c4055cfc0d6c37e52e6d7ef7ad8ef3e6117b062aa8
LIQUIDLANE_VAULT_TYPE_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x1
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=0x6136759bd57001401d19369a4de524f9a19b8839e97a0fea107e308fe98b7b29
LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x2
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=0x2ecd2ec19a0be5707dbf61cd8bd758d84735beb0520a64addd9fe6a89772226d
LIQUIDLANE_REQUEST_TYPE_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x3
LIQUIDLANE_FUNDING_INTENT_TYPE_CODE_HASH=0x4c5ecdd444594253667e19b9a473e712b5c77973159d94620cd5c2a86f3d3c45
LIQUIDLANE_FUNDING_INTENT_TYPE_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x4
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=0xb3c57c3ec41803ea125250ffc51f30873e60fa827ef55b382b0c36ee9fcd4240
LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT=0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1#0x5
LIQUIDLANE_EXECUTOR_CKB_ADDRESS=ckt1qyqxqf7spwqfwlqtyccswl0376fagku9el5q75f2vl
LIQUIDLANE_VAULT_CKB_ADDRESS=ckt1qqlh00rsw509klehscl6nrglnvsh6m69765ve0cdtznxjdmr9h9c2qjkg67zcsv3rnhkmqqpfp7p8x7hglmkq6hfm56s4gma8zvlfp9xd3csexlpk32wp07v542knpx897let0myljq2x8ldqrcmf8e42jv5xcfkwkda2uqpgqw3jd56fhjjf7dpnwyrn6t6pl4pql3s3l5ck7ef9mxjasv6p0jhqldlv8xch46cmprnt04s2g9xftwanln239mjyfk5chkd63z9jsjnvelpnwdyw0n39dw809e3t8v5vgxdts4gdu7nc3dnc47ra3qcq04py5jsllz37vy88es04qn774dns2cvxmhfln2zgqm26tzj
LIQUIDLANE_VAULT_CELL_OUT_POINT=0xaa40c3232ff92247a4edffd7e166e2691020b8bf2688cef67f6efc87bbce8f36#0x0
```

Explorer links:

- Script deployment: https://pudge.explorer.nervos.org/transaction/0xc13f6900ab4d5007cd0307fc3fe2f3b57c666dcf671e02a39c3ddc5b26b249b1
- Fresh active vault: https://pudge.explorer.nervos.org/transaction/0xaa40c3232ff92247a4edffd7e166e2691020b8bf2688cef67f6efc87bbce8f36

## End-To-End Test

1. Confirm `/health` has `external_funding_ready=true`.
2. Supply CKB from an LP wallet.
3. Reserve capacity from a merchant wallet and pay the lease fee.
4. Confirm the request tx appears on Pudge explorer.
5. Confirm Fiber invokes Core's funding builder during handoff.
6. Confirm the funding tx appears on Pudge explorer with the Fiber funding lock output.
7. Confirm the request moves to `channel_open` only after Fiber `list_channels` reports the channel active.
8. Confirm LP portfolio shows reserved/deployed balances and lease fees correctly segmented by wallet.
