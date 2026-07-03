#!/usr/bin/env bash
set -euo pipefail

root="${RISCV_TOOLCHAIN_ROOT:-/tmp/liquidlane-riscv-toolchain/root}"
work="${RISCV_TOOLCHAIN_WORK:-/tmp/liquidlane-riscv-toolchain/packages}"
mkdir -p "$root" "$work"

if command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
  printf 'riscv64-unknown-elf-gcc already available at %s\n' "$(command -v riscv64-unknown-elf-gcc)"
  exit 0
fi

if [[ -x "$root/usr/bin/riscv64-unknown-elf-gcc" ]]; then
  printf 'user-space RISC-V toolchain available at %s/usr/bin\n' "$root"
  exit 0
fi

if ! command -v apt-get >/dev/null 2>&1 || ! command -v dpkg >/dev/null 2>&1; then
  printf 'apt-get and dpkg are required for this helper. Install riscv64-unknown-elf-gcc manually.\n' >&2
  exit 1
fi

(
  cd "$work"
  apt-get download gcc-riscv64-unknown-elf binutils-riscv64-unknown-elf
  for pkg in ./*riscv64-unknown-elf*.deb; do
    dpkg -x "$pkg" "$root"
  done
)

printf 'RISC-V toolchain extracted. Use:\n'
printf '  export RISCV_TOOLCHAIN_BIN=%s/usr/bin\n' "$root"
