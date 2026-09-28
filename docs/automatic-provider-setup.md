# Automatic provider setup

Supply Liquidity now supports **create node → background operation → add CKB → publish offer**. The first installation requires a local command. After that, Fiber and the connector run as Linux user services; the browser moves to Add capital when the paired node reports a fresh background-service heartbeat.

## Start from this workspace

Run from `liquidlane-core` in a normal local terminal:

```sh
bash scripts/start-local-testnet.sh
```

This starts the app on **http://localhost:3000** and the coordinator on port 18180, using the existing `runtime/testnet/core.sqlite3`. It refuses to replace a missing database or take over an occupied port. Keep this terminal open. Existing participant nodes keep their own startup arrangements.

In Supply Liquidity, create a fresh setup, select **Create a node & run automatically**, and download the connector settings. Then run in another terminal:

```sh
bash scripts/setup-provider.sh
```

The helper builds the current connector and uses `~/Downloads/liquidlane-pairing.json`. Keep this checkout and its built binary in place: the local services use it. Setup prints a status command with the exact binary and configuration paths, so an older globally installed connector is not needed. To use another downloaded file, pass its real path as the first argument. Pairing codes last ten minutes. An expired file is rejected; download a fresh one. Repeating setup uses the same managed node for that wallet and role, preserving its keys and journal.

## Install on another node machine

Requires Linux x86_64, a working systemd user session, Rust 1.91 or later, `tar`, and access to GitHub, CKB testnet, and your coordinator. Install the connector from this repository **before** creating the pairing code:

```sh
cargo install --locked --path . --bin liquidlane-connector
liquidlane-connector setup ./liquidlane-pairing.json --new-node --background
```

The second command runs from the download folder. Setup verifies the pinned official Fiber 0.9.0 archive, creates an unfunded node, starts Fiber, verifies its identity/network, pairs it, and starts its connector. It requires no RPC URL or key-path input. Setup prints the funding address and absolute connector configuration path. Compare the funding address with Add capital, then sign your CKB transfer in your browser wallet. A published offer needs confirmed available capital; for a minimum 500 CKB offer the baseline is 663 CKB before existing reservations.

For an existing running node, omit `--new-node` and run setup from its existing connector configuration folder. Stop a manually running connector first. This enables background operation for the connector; the existing Fiber process still needs its own service. On other operating systems, use normal `setup` and keep the process running.

## Local operation and security

- The node data lives under `$XDG_DATA_HOME/liquidlane/nodes` or `~/.local/share/liquidlane/nodes`. The setup directory contains `connector.json`, `connector-state`, native Fiber data, the CKB wallet, and a private startup credential. Back up the native state and keys together using Fiber's procedures. Existing node files are never replaced with new keys.
- Keys and the startup credential remain on the operator's machine. Directories use mode 0700 and credential files 0600. This is an online wallet: software running as that user, or root, can access it. The password is loaded locally, never sent to Core or put in command arguments or unit files.
- Fiber RPC listens only on loopback. Managed launch removes inherited configuration overrides. Provider incoming auto-funding is disabled; marketplace openings still require both parties' signatures and the connector's local spending limits.
- The TCP peer address uses the machine's actual network interface. A private address works on the same network; NAT/firewalls may require a reachable announced address for remote peers. Automatic installation does not prove public routing or create the separate payment route needed for delivery testing.
- Both services restart after failure. Setup tries to enable startup before login and reports whether this works. Otherwise startup is enabled at login. Keep the machine powered on. An existing connector process is detected before its configuration/token is changed.

From the configuration folder printed by setup:

```sh
liquidlane-connector service-status ./connector.json
liquidlane-connector service-stop ./connector.json
liquidlane-connector service-start ./connector.json
```

`service-stop` disables only the connector. Fiber stays running to manage existing channels. Pausing an offer stops new requests; previously accepted orders and native channel settlement remain separate.

## Verification boundary

The [28 September validation](evidence/2026-09-28/README.md) uses a fresh, unfunded managed node and an isolated coordinator with real CKB RPC, native wallet signatures, and systemd user services. It checks the official release digest, setup, signed funding heartbeat, both services recovering from forced process crashes, explicit service restart, and repeated setup preserving keys and identity. The browser check verifies the real signed funding address and Add capital page on desktop and mobile. No channel or funding transfer is requested by this check.

Reproduce after building the Core binaries and starting the app (with its dependencies and Playwright Chromium installed):

```sh
cargo build --locked --bins
python3 scripts/verify-provider-setup.py --app-url http://localhost:3000 --output /tmp/provider-services.json
```

The check copies the binaries to stable temporary paths, installs only its own temporary services, and removes those services and their unfunded keys afterward. It restores the previous user linger setting. `--cache-home /absolute/cache/path` can retain the public archive for subsequent runs; the installer still checks the pinned SHA-256. `PLAYWRIGHT_CHROMIUM_PATH` optionally selects an already installed Chromium executable. Omit `--app-url` to check only the native services.

A full machine reboot and a JoyID passkey-signed capital transfer into a newly managed node remain manual validation items. Enabled startup units, linger, and process/service restart checks do not establish a real reboot result. The native-wallet browser session in this check is not a JoyID passkey login; the 19 wallet/payment regressions separately cover signed-address verification, transaction validation, and safe payment retries.
