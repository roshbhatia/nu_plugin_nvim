#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if rg -n 'nvim[-_](oxi|rs)|mlua' \
  Cargo.toml crates/*/Cargo.toml crates/*/src --glob '!generated.rs' ||
  rg -n "require\\(['\"]nu['\"]\\)" crates/*/src --glob '!generated.rs'; then
  echo "editor-side bridge dependency or source found" >&2
  exit 1
fi
