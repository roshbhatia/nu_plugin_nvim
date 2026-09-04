#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
test_root=$(mktemp -d)
test_socket="$test_root/nvim.sock"
test_plugin=${NUVIM_TEST_PLUGIN:-}
test_pid=

cleanup() {
  if [[ -n $test_pid ]]; then
    kill "$test_pid" 2> /dev/null || true
    wait "$test_pid" 2> /dev/null || true
  fi
  rm -rf "$test_root"
}
trap cleanup EXIT

if [[ -z $test_plugin ]]; then
  cargo build -p nu-plugin-nuvim
  test_plugin="$repo_root/target/debug/nu_plugin_nuvim"
fi

mkdir -p "$test_root/config/nushell" "$test_root/data"
touch "$test_root/config/nushell/config.nu"

nvim --headless --clean -n -u NONE --listen "$test_socket" \
  < /dev/null > "$test_root/nvim.log" 2>&1 &
test_pid=$!

for _attempt in {1..100}; do
  if [[ -S $test_socket ]]; then
    break
  fi
  if ! kill -0 "$test_pid" 2> /dev/null; then
    cat "$test_root/nvim.log" >&2
    exit 1
  fi
  sleep 0.05
done

if [[ ! -S $test_socket ]]; then
  cat "$test_root/nvim.log" >&2
  echo "Neovim did not create $test_socket" >&2
  exit 1
fi

env \
  NUVIM_TEST_PLUGIN="$test_plugin" \
  NUVIM_TEST_SERVER="$test_socket" \
  NUVIM_TEST_FILE="$repo_root/README.md" \
  XDG_CONFIG_HOME="$test_root/config" \
  XDG_DATA_HOME="$test_root/data" \
  nu --no-config-file --plugins "$test_plugin" \
  "$repo_root/tests/agent-control.nu"
