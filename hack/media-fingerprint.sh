#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

{
  printf '%s\n' Cargo.lock Cargo.toml README.md
  find crates -type f \( -name '*.rs' -o -name 'Cargo.toml' \) -print
  printf '%s\n' \
    hack/nuvim.tape \
    hack/screenshot-diagnostics.lua \
    hack/screenshot.nu \
    hack/screenshots.sh
} | LC_ALL=C sort | xargs sha256sum | sha256sum | cut -d ' ' -f 1
