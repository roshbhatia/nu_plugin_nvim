#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

expected=$(cat docs/media.sha256)
actual=$(./hack/media-fingerprint.sh)
if [[ $actual != "$expected" ]]; then
  echo "docs media is stale; run ./hack/screenshots.sh" >&2
  exit 1
fi

read -r png_format png_width png_height < <(
  magick identify -format '%m %w %h\n' docs/nuvim.png
)
if [[ $png_format != PNG || $png_width -lt 1000 || $png_height -lt 300 ]]; then
  echo "docs/nuvim.png has an unexpected format or size" >&2
  exit 1
fi

read -r gif_format gif_width gif_height < <(
  magick identify -format '%m %w %h\n' 'docs/nuvim.gif[0]'
)
gif_frames=$(magick identify docs/nuvim.gif | wc -l | tr -d ' ')
if [[ $gif_format != GIF || $gif_width -ne 1400 || $gif_height -ne 720 || $gif_frames -lt 5 ]]; then
  echo "docs/nuvim.gif has an unexpected format, size, or frame count" >&2
  exit 1
fi
