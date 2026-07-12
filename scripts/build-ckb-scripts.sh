#!/usr/bin/env bash
set -euo pipefail

target="riscv64imac-unknown-none-elf"
out_dir="ckb-scripts/build"
tool_bin="${RISCV_TOOLCHAIN_BIN:-}"

if [[ -n "$tool_bin" ]]; then
  export PATH="$tool_bin:$PATH"
elif [[ -x /tmp/liquidlane-riscv-toolchain/root/usr/bin/riscv64-unknown-elf-gcc ]]; then
  export PATH="/tmp/liquidlane-riscv-toolchain/root/usr/bin:$PATH"
fi

if ! rustup target list --installed | grep -qx "$target"; then
  rustup target add "$target"
fi

export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=-a"

for tool in riscv64-unknown-elf-gcc riscv64-unknown-elf-strip; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '%s is required. Run scripts/setup-riscv-toolchain.sh or install the RISC-V toolchain.\n' "$tool" >&2
    exit 1
  fi
done

cargo build --manifest-path ckb-scripts/Cargo.toml --release --target "$target"
mkdir -p "$out_dir"

for script in \
  liquidlane-vault-lock \
  liquidlane-vault-type \
  liquidlane-lp-receipt-type \
  liquidlane-capacity-request-type \
  liquidlane-funding-intent-type \
  liquidlane-fee-claim-type; do
  cp "ckb-scripts/target/$target/release/$script" "$out_dir/$script"
  riscv64-unknown-elf-strip "$out_dir/$script"
done

scripts/generate-ckb-script-manifest.py --artifacts "$out_dir" --network "${LIQUIDLANE_CKB_NETWORK:-testnet}"
