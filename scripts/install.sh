#!/usr/bin/env bash
set -euo pipefail

PREFIX_BIN="${HOME}/.local/bin"
PREFIX_APP="${HOME}/.local/share/applications"
PREFIX_ICON="${HOME}/.local/share/icons/hicolor/scalable/apps"

if [[ "${1:-}" == "--uninstall" ]]; then
  echo "Removing Chronos binary and desktop assets (preserving user data in ~/.local/share/chronos)..."
  rm -f "${PREFIX_BIN}/chronos"
  rm -f "${PREFIX_APP}/chronos.desktop"
  rm -f "${PREFIX_ICON}/chronos.svg"
  echo "Uninstalled cleanly."
  exit 0
fi

echo "Building Chronos release binary with Cargo..."
cargo build --release --locked

mkdir -p "${PREFIX_BIN}" "${PREFIX_APP}" "${PREFIX_ICON}"
install -m 755 "target/release/chronos" "${PREFIX_BIN}/chronos"
install -m 644 "assets/chronos.desktop" "${PREFIX_APP}/chronos.desktop"
install -m 644 "assets/chronos.svg" "${PREFIX_ICON}/chronos.svg"

echo "Chronos installed to ${PREFIX_BIN}/chronos"
