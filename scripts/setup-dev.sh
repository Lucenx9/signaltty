#!/usr/bin/env bash
# Install the reference Ubuntu environment; safe to repeat.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-}" in
  --system)
    source /etc/os-release
    if [[ "$ID" != ubuntu || "$VERSION_ID" != 26.04 ]]; then
      echo 'System package installation supports Ubuntu 26.04; see scripts/dev-packages.txt for other distributions.' >&2
      exit 1
    fi
    privilege=()
    if [[ "$EUID" -ne 0 ]]; then privilege=(sudo); fi
    mapfile -t packages < "$root/scripts/dev-packages.txt"
    "${privilege[@]}" apt-get update
    "${privilege[@]}" env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${packages[@]}"
    ;;
  '') ;;
  *) echo 'Usage: scripts/setup-dev.sh [--system]' >&2; exit 2 ;;
esac
if ! command -v rustup >/dev/null; then
  echo 'Install rustup from https://rustup.rs, then rerun this command.' >&2
  exit 1
fi
cd "$root"
toolchain="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)"
rustup toolchain install "$toolchain" --profile minimal --component rustfmt --component clippy
scripts/verify.sh doctor
