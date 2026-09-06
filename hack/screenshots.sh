#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

fixture=$(mktemp -d)
editor_pid=""
cleanup() {
  if [[ -n $editor_pid ]]; then
    kill "$editor_pid" 2> /dev/null || true
  fi
  rm -rf "$fixture"
}
trap cleanup EXIT

mkdir -p "$repo_dir/docs"
package=$(nix build --no-link --print-out-paths)
server="$fixture/nvim.sock"

nvim --headless --clean --listen "$server" README.md \
  "+filetype on" \
  "+edit README.md" \
  "+call append(0, '-- demo edit')" \
  "+luafile hack/screenshot-diagnostics.lua" \
  > "$fixture/nvim.log" 2>&1 &
editor_pid=$!

for _ in {1..50}; do
  [[ -S $server ]] && break
  sleep 0.1
done
[[ -S $server ]]

export NVIM="$server"
export NUVIM_SCREENSHOT_REPO="$repo_dir"
screenshot_output="$fixture/nuvim.ansi"
if ! timeout 30s nu --no-config-file --no-history \
  --plugins "$package/bin/nu_plugin_nuvim" \
  hack/screenshot.nu > "$screenshot_output"; then
  echo "Nuvim screenshot capture failed" >&2
  exit 1
fi
if [[ ! -s $screenshot_output ]]; then
  echo "Nuvim screenshot capture produced no output" >&2
  exit 1
fi
for expected in Nuvim buffers diagnostics README.md; do
  if ! grep -Fq "$expected" "$screenshot_output"; then
    echo "Nuvim screenshot capture omitted $expected" >&2
    exit 1
  fi
done
timeout 30s freeze \
  --language ansi \
  - \
  --output "$repo_dir/docs/nuvim.png" \
  --width 1100 \
  --padding 24 \
  --margin 16 \
  --window < "$screenshot_output"
if [[ ! -s $repo_dir/docs/nuvim.png ]]; then
  echo "Nuvim screenshot render produced no output" >&2
  exit 1
fi
identify "$repo_dir/docs/nuvim.png" > /dev/null

plugin_dir="$fixture/config/nushell"
mkdir -p "$plugin_dir"
printf '\044%s\n' 'env.config.show_banner = false' > "$plugin_dir/config.nu"
nu --config /dev/null --env-config /dev/null --plugin-config "$plugin_dir/plugin.msgpackz" \
  -c "plugin add '$package/bin/nu_plugin_nuvim'"
if ! XDG_CONFIG_HOME="$fixture/config" timeout 60s \
  vhs hack/nuvim.tape --output "$repo_dir/docs/nuvim.gif"; then
  echo "Nuvim demo capture failed" >&2
  exit 1
fi
if [[ ! -s $repo_dir/docs/nuvim.gif ]]; then
  echo "Nuvim demo capture produced no output" >&2
  exit 1
fi
identify "$repo_dir/docs/nuvim.gif" > /dev/null
./hack/media-fingerprint.sh > "$repo_dir/docs/media.sha256"
