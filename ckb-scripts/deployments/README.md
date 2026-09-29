# Legacy deployment records

Public CKB testnet records for the separate legacy vault runtime. Each filename includes its deployment transaction prefix. The JSON records retain full transaction hashes, script hashes, outpoints, explorer links, and original deployment timestamps.

These records establish historical deployments. Check the current chain state before using a code or vault outpoint; a recorded vault cell may already be spent.

## Script deployments

- [testnet-1ef53454fed7.json](testnet-1ef53454fed7.json)
- [testnet-516faf4a750c.json](testnet-516faf4a750c.json)
- [testnet-a00be7fdb859.json](testnet-a00be7fdb859.json)
- [testnet-a328147c40b9.json](testnet-a328147c40b9.json)
- [testnet-c13f6900ab4d.json](testnet-c13f6900ab4d.json)
- [testnet-eedb45c8ebf8.json](testnet-eedb45c8ebf8.json)

## Vault deployments

- [vault-testnet-05bfc0fa84b5.json](vault-testnet-05bfc0fa84b5.json)
- [vault-testnet-477be93d5587.json](vault-testnet-477be93d5587.json)
- [vault-testnet-a63bd78a94bc.json](vault-testnet-a63bd78a94bc.json)
- [vault-testnet-aa40c3232ff9.json](vault-testnet-aa40c3232ff9.json)
- [vault-testnet-ae1654e88c3b.json](vault-testnet-ae1654e88c3b.json)

## Recording a deployment

Use [testnet.template.json](testnet.template.json) for a public record, or retain the JSON written by the deployment CLI. The CLI names records `testnet-<transaction-prefix>.json` and `vault-testnet-<transaction-prefix>.json`. Build instructions and the legacy runtime boundary are in the [script guide](../README.md).

Local builds produce artifacts under the ignored `ckb-scripts/build` directory. They do not create public transaction hashes or explorer records. Keep local chain records as `*.local.json`, and keep keys, RPC credentials, and wallet exports outside version control.
