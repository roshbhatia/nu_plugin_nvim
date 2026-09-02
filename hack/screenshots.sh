#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

fixture=$(mktemp -d)
editor_pid=""
cleanup() {
  if [[ -n "$editor_pid" ]]; then
    kill "$editor_pid" 2>/dev/null || true
  fi
  rm -rf "$fixture"
}
trap cleanup EXIT

mkdir -p "$repo_dir/docs"
package=$(nix build --no-link --print-out-paths)
server="$fixture/nvim.sock"

nvim --headless --clean --listen "$server" README.md plugin/nu.lua \
  "+filetype on" \
  "+call append(0, '-- demo edit')" \
  >"$fixture/nvim.log" 2>&1 &
editor_pid=$!

for _ in {1..50}; do
  [[ -S "$server" ]] && break
  sleep 0.1
done
[[ -S "$server" ]]

export NVIM="$server"
export NUVIM_SCREENSHOT_REPO="$repo_dir"
freeze \
  --execute "nu --no-config-file --no-history --plugins $package/bin/nu_plugin_nuvim hack/screenshot.nu" \
  --output "$repo_dir/docs/nuvim.png" \
  --width 1100 \
  --padding 24 \
  --margin 16 \
  --window
