#!/usr/bin/env python3
"""Exercise real, unfunded managed services against an isolated coordinator.

Requires built debug binaries, Linux user systemd, and public testnet/GitHub access.
Creates temporary keys/services, never requests channels or transfers, and removes
only its own services. Restores the user's original linger setting on exit.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def command(args, **kwargs):
    kwargs.setdefault("timeout", 180)
    return subprocess.run([str(s) for s in args], check=True, text=True,
                          capture_output=True, **kwargs).stdout.strip()


def control(*args):
    return command(["systemctl", "--user", "--no-ask-password", *args])


def request(url, body=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = None if body is None else json.dumps(body).encode()
    with urllib.request.urlopen(urllib.request.Request(url, data=data, headers=headers), timeout=10) as response:
        return json.load(response)


def eventually(work, timeout=60):
    deadline = time.monotonic() + timeout
    error = None
    while time.monotonic() < deadline:
        try:
            result = work()
            if result:
                return result
        except (OSError, ValueError, subprocess.SubprocessError) as failure:
            error = failure
        time.sleep(1)
    raise RuntimeError(f"condition not reached: {error or 'timed out'}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ckb-rpc", default="https://testnet.ckb.dev/rpc")
    parser.add_argument("--cache-home", type=Path, help="reuse the public archive cache; its pinned digest is still verified")
    parser.add_argument("--app-url", help="also verify the live Add capital page in Chromium")
    args = parser.parse_args()
    output = args.output.resolve()
    folder = Path(tempfile.mkdtemp(prefix="liquidlane-provider-check-"))
    folder.chmod(0o700)
    binaries = folder / "bin"
    binaries.mkdir()
    for name in ["liquidlane-core", "liquidlane-wallet", "liquidlane-connector"]:
        # A concurrent cargo build replaces target/debug binaries. Services need
        # stable executable paths throughout installation and crash recovery.
        shutil.copy2(ROOT / "target/debug" / name, binaries / name)
    env = dict(os.environ, XDG_DATA_HOME=str(folder / "data"),
               XDG_CACHE_HOME=str(args.cache_home.resolve() if args.cache_home else folder / "cache"),
               RUST_BACKTRACE="1")
    user = command(["id", "-un"])
    linger = command(["loginctl", "show-user", user, "-p", "Linger", "--value"])
    core = None
    report = {"unfunded_test": True, "host_reboot_tested": False, "joyid_passkey_tested": False}
    try:
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        url = f"http://127.0.0.1:{port}"
        core_env = dict(env, LIQUIDLANE_PRODUCT_MODE="marketplace", LIQUIDLANE_ENV="test",
                        LIQUIDLANE_BIND_ADDR=f"127.0.0.1:{port}",
                        LIQUIDLANE_MARKET_ORIGIN=args.app_url or "http://localhost:3000",
                        LIQUIDLANE_MARKET_DB=str(folder / "market.sqlite3"),
                        LIQUIDLANE_CKB_RPC_URL=args.ckb_rpc)
        with (folder / "core.log").open("w") as log:
            core = subprocess.Popen([binaries / "liquidlane-core"], cwd=folder,
                                    env=core_env, stdout=log, stderr=log)
        eventually(lambda: request(url + "/health"))
        key = folder / "account.key"
        key.write_bytes(os.urandom(32))
        key.chmod(0o600)
        wallet = binaries / "liquidlane-wallet"
        connector = binaries / "liquidlane-connector"
        owner = command([wallet, "address", key], cwd=folder)
        session = folder / "session.json"
        command([wallet, "login", key, url, session], cwd=folder)
        token = json.loads(session.read_text())["token"]

        def api(path, body=None):
            return request(url + path, body, token)

        def setup():
            pair = api("/market/nodes/pair", {"role": "provider", "label": "Isolated service check"})
            pairing = {"format": "liquidlane-pairing/1", "core_url": url, "ckb_rpc": args.ckb_rpc,
                       "owner_address": owner, "pairing_code": pair["pairing_code"],
                       "expires_at": pair["expires_at"], "role": "provider",
                       "provider_policy": {"max_order_ckb": 2000, "max_total_ckb": 5000,
                                           "min_fee_ckb": 61, "accept_public_orders": True,
                                           "auto_approve": True, "allowed_merchants": []}}
            path = folder / "pairing.json"
            path.write_text(json.dumps(pairing))
            path.chmod(0o600)
            try:
                log = command([connector, "setup", path, "--new-node", "--background"], cwd=folder, env=env, timeout=480)
                (folder / "setup.log").write_text(log)
            except subprocess.CalledProcessError as error:
                (folder / "setup.log").write_text(error.stdout + error.stderr)
                raise

        print("Installing an isolated unfunded provider", flush=True)
        setup()
        metadata_path = next((folder / "data/liquidlane/nodes").glob("*/managed-node.json"))
        metadata = json.loads(metadata_path.read_text())
        directory = metadata_path.parent
        service = json.loads((directory / "connector-state/service.json").read_text())
        units = [metadata["fiber_unit"], service["unit"]]

        def heartbeat(after=0):
            nodes = api("/market/dashboard")["nodes"]
            return next((n for n in nodes if n["background"] and n["last_seen"] > after and n.get("funding")), None)

        node = eventually(heartbeat)
        assert node["available_ckb"] == 0 and node["provider_policy"]["committed_ckb"] == 0
        assert api("/market/dashboard")["orders"] == []
        identity = node["pubkey"]
        files = [directory / name for name in ["ckb/key", "fiber/sk", "fiber-password"]]
        digests = [hashlib.sha256(p.read_bytes()).hexdigest() for p in files]
        assert all(p.stat().st_mode & 0o077 == 0 for p in files)
        info = request(metadata["fiber_rpc"], {"jsonrpc": "2.0", "id": 1, "method": "node_info", "params": []})["result"]
        assert info["version"] == "0.9.0" and int(info["auto_accept_channel_ckb_funding_amount"], 16) == 0
        assert all(control("is-enabled", unit) == "enabled" for unit in units)
        report.update(fresh_install=True, signed_funding_heartbeat=True, incoming_funding_disabled=True,
                      private_keys=True, unit_startup_enabled=True, starts_at_boot=service["starts_at_boot"])
        print("Checking automatic restart after each service crashes", flush=True)
        for unit in units:
            pid = control("show", unit, "-p", "MainPID", "--value")
            control("kill", "--kill-whom=main", "--signal=SIGKILL", unit)
            eventually(lambda: control("show", unit, "-p", "MainPID", "--value") not in [pid, "0"])
            assert int(control("show", unit, "-p", "NRestarts", "--value")) > 0
        node = eventually(lambda: heartbeat(node["last_seen"]))
        assert node["pubkey"] == identity
        report["crash_restart_preserved_identity"] = True
        control("restart", *units)
        node = eventually(lambda: heartbeat(node["last_seen"]))
        assert node["pubkey"] == identity
        report["service_restart_preserved_identity"] = True
        print("Checking repeated setup preserves the same node and keys", flush=True)
        setup()
        node = eventually(lambda: heartbeat(node["last_seen"]))
        assert node["pubkey"] == identity
        assert [hashlib.sha256(p.read_bytes()).hexdigest() for p in files] == digests
        assert len(api("/market/dashboard")["nodes"]) == 1
        report["repeat_setup_preserved_identity_and_keys"] = True
        if args.app_url:
            browser = command(["node", ROOT.parent / "liquidlane-app/scripts/verify-provider-browser.mjs"],
                input=json.dumps({"coreURL":url,"appURL":args.app_url,"owner":owner,
                    "session":json.loads(session.read_text()),"address":node["funding"]["address"],
                    "screenshot":str(output.with_suffix(".png"))}), env=env)
            report["live_capital_page"] = json.loads(browser)
        report["observed_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2), flush=True)
    except Exception as error:
        # Preserve diagnostics, excluding all keys/configuration/session tokens.
        output.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(error, subprocess.CalledProcessError):
            output.with_suffix(".failure.log").write_text((error.stdout or "") + (error.stderr or ""))
        for name in ["core.log", "setup.log"]:
            if (folder / name).exists():
                shutil.copyfile(folder / name, output.with_suffix("." + name))
        raise
    finally:
        units = set()
        for path in (folder / "data/liquidlane/nodes").glob("*/managed-node.json"):
            units.add(json.loads(path.read_text())["fiber_unit"])
        for path in (folder / "data/liquidlane/nodes").glob("*/connector-state/service.json"):
            units.add(json.loads(path.read_text())["unit"])
        # The unit name is deterministic even if setup failed before service.json.
        for path in (folder / "data/liquidlane/nodes").glob("*/connector-state"):
            digest = hashlib.sha256(str(path.resolve()).encode()).hexdigest()[:24]
            units.add(f"liquidlane-connector-{digest}.service")
        config = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config")))
        for unit in units:
            subprocess.run(["systemctl", "--user", "disable", "--now", unit], capture_output=True, timeout=60)
            (config / "systemd/user" / unit).unlink(missing_ok=True)
        if units:
            control("daemon-reload")
            subprocess.run(["systemctl", "--user", "reset-failed", *units], capture_output=True, timeout=15)
        if linger == "no":
            command(["loginctl", "--no-ask-password", "disable-linger", user])
        if core:
            core.send_signal(signal.SIGINT)
            core.wait(timeout=20)
        shutil.rmtree(folder)


if __name__ == "__main__":
    main()
