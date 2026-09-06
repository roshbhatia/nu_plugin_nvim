#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

hashes=$(sed -n 's/^[[:space:]]*cargoHash = "\([^"]*\)";.*/\1/p' flake.nix)
if [[ -z $hashes ]]; then
  echo "flake.nix has no cargoHash values" >&2
  exit 1
fi

old_hash=$(sed -n '1p' <<< "$hashes")
if grep -Fvx "$old_hash" <<< "$hashes" > /dev/null; then
  echo "flake.nix cargoHash values do not match" >&2
  exit 1
fi

fake_hash="sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
tmp_dir=$(mktemp -d)
backup="$tmp_dir/flake.nix"
tmp_file="$tmp_dir/updated.nix"
cp flake.nix "$backup"
complete=false
cleanup() {
  if [[ $complete != true ]]; then
    cp "$backup" flake.nix
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

while IFS= read -r line || [[ -n $line ]]; do
  printf '%s\n' "${line//$old_hash/$fake_hash}"
done < flake.nix > "$tmp_file"
mv "$tmp_file" flake.nix

set +e
build_output=$(nix build .#runtime --no-link --print-build-logs 2>&1)
build_status=$?
set -e

if [[ $build_status -eq 0 ]]; then
  echo "nix accepted the fake cargoHash" >&2
  exit 1
fi

new_hash=$(sed -n 's/^[[:space:]]*got:[[:space:]]*\(sha256-[A-Za-z0-9+\/=]*\).*/\1/p' <<< "$build_output" | tail -n 1)
if [[ -z $new_hash ]]; then
  printf '%s\n' "$build_output" >&2
  echo "could not read the expected cargoHash from nix build" >&2
  exit 1
fi

while IFS= read -r line || [[ -n $line ]]; do
  printf '%s\n' "${line//$fake_hash/$new_hash}"
done < flake.nix > "$tmp_file"
mv "$tmp_file" flake.nix
complete=true

printf 'updated cargoHash to %s\n' "$new_hash"
