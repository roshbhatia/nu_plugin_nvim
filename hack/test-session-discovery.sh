#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
test_root=$(mktemp -d)
runtime_dir="$test_root/runtime"
test_plugin=${NUVIM_TEST_PLUGIN:-}
first_pid=
second_pid=

cleanup() {
  for process_id in "$first_pid" "$second_pid"; do
    if [[ -n $process_id ]]; then
      kill "$process_id" 2> /dev/null || true
      wait "$process_id" 2> /dev/null || true
    fi
  done
  rm -rf "$test_root"
}
trap cleanup EXIT

if [[ -z $test_plugin ]]; then
  cargo build -p nu-plugin-nuvim
  test_plugin="$repo_root/target/debug/nu_plugin_nuvim"
fi

mkdir -p "$runtime_dir" "$test_root/config/nushell" "$test_root/data"
touch "$test_root/config/nushell/config.nu"
touch "$runtime_dir/nvim.stale.0"

start_editor() {
  local socket=$1
  local path=$2
  nvim --headless --clean -n -u NONE --listen "$socket" "$path" \
    < /dev/null > "$test_root/nvim.log" 2>&1 &
  local process_id=$!
  for _attempt in {1..100}; do
    [[ -S $socket ]] && break
    if ! kill -0 "$process_id" 2> /dev/null; then
      cat "$test_root/nvim.log" >&2
      return 1
    fi
    sleep 0.05
  done
  [[ -S $socket ]]
  printf '%s\n' "$process_id"
}

run_nu() {
  env -u NVIM \
    XDG_CONFIG_HOME="$test_root/config" \
    XDG_DATA_HOME="$test_root/data" \
    XDG_RUNTIME_DIR="$runtime_dir" \
    nu --no-config-file --plugins "$test_plugin" -c "$1"
}

assert_cross_session_error() {
  local output=$1
  local pattern
  for pattern in \
    "Neovim handle belongs to server" \
    "runtime/nvim.one.0" \
    "not target server" \
    "runtime/nvim.two.0"; do
    if ! grep -F "$pattern" "$output" > /dev/null; then
      cat "$output" >&2
      return 1
    fi
  done
}

first_socket="$runtime_dir/nvim.one.0"
first_pid=$(start_editor "$first_socket" "$test_root/one.md")

selected=$(run_nu 'nuvim | get server')
[[ $selected == "$first_socket" ]]
count=$(run_nu 'nuvim servers | length')
[[ $count == 1 ]]

second_socket="$runtime_dir/nvim.two.0"
second_pid=$(start_editor "$second_socket" "$test_root/two.md")

selected=$(
  NVIM="$first_socket" \
    XDG_CONFIG_HOME="$test_root/config" \
    XDG_DATA_HOME="$test_root/data" \
    XDG_RUNTIME_DIR="$runtime_dir" \
    nu --no-config-file --plugins "$test_plugin" -c 'nuvim | get server'
)
[[ $selected == "$first_socket" ]]
selected=$(run_nu "nuvim --server '$second_socket' | get server")
[[ $selected == "$second_socket" ]]

direct_cross_session="let handle = (nuvim call nvim_get_current_buf --server '$first_socket'); nuvim call nvim_buf_get_name \$handle --server '$second_socket'"
if run_nu "$direct_cross_session" > "$test_root/direct-cross-session.out" 2>&1; then
  echo "nuvim accepted a raw handle from another server" >&2
  exit 1
fi
assert_cross_session_error "$test_root/direct-cross-session.out"

nested_cross_session="let handle = (nuvim call nvim_get_current_buf --server '$first_socket'); {payload: [\$handle]} | nuvim lua 'return ...' --server '$second_socket'"
if run_nu "$nested_cross_session" > "$test_root/nested-cross-session.out" 2>&1; then
  echo "nuvim accepted a nested pipeline handle from another server" >&2
  exit 1
fi
assert_cross_session_error "$test_root/nested-cross-session.out"

export NUVIM_PICKER_CONFIG="$test_root/config"
export NUVIM_PICKER_DATA="$test_root/data"
export NUVIM_PICKER_PLUGIN="$test_plugin"
export NUVIM_PICKER_RUNTIME="$runtime_dir"
expect << 'EXPECT'
  set timeout 15
  spawn env -u NVIM XDG_CONFIG_HOME=$env(NUVIM_PICKER_CONFIG) XDG_DATA_HOME=$env(NUVIM_PICKER_DATA) XDG_RUNTIME_DIR=$env(NUVIM_PICKER_RUNTIME) nu --no-config-file --plugins $env(NUVIM_PICKER_PLUGIN) -c {let selected = (nuvim); print $"SELECTED=($selected.server)"}
  expect "Select a Neovim session"
  send "two\r"
  expect -re {SELECTED=.*nvim\.two\.0}
  expect eof
  catch wait result
  exit [lindex $result 3]
EXPECT

kill "$first_pid" "$second_pid"
wait "$first_pid" "$second_pid" 2> /dev/null || true
first_pid=
second_pid=

if run_nu 'nuvim' > "$test_root/missing.out" 2>&1; then
  echo "nuvim unexpectedly selected a stale server" >&2
  exit 1
fi
grep -F "no live Neovim server found" "$test_root/missing.out" > /dev/null
