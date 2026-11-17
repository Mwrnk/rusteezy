#!/usr/bin/env bash
# Record an example and turn it into a GIF for the README.
#
#   scripts/gif.sh <example> <start-seconds> <duration-seconds> [extra example args...]
#
# Needs ffmpeg. Output: docs/gifs/<example>.gif
set -euo pipefail

example="$1"; start="$2"; seconds="$3"; shift 3
fps="${GIF_FPS:-15}"
width="${GIF_WIDTH:-640}"
colors="${GIF_COLORS:-128}"
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cd "$root"
mkdir -p docs/gifs
cargo build --release --example "$example" >/dev/null
"target/release/examples/$example" --size 760x480 --no-hud \
    --record "$tmp/frames.rgb" --fps "$fps" --start "$start" --seconds "$seconds" "$@"

read -r w h rec_fps < "$tmp/frames.rgb.txt"
ffmpeg -loglevel error -y \
    -f rawvideo -pixel_format rgb24 -video_size "${w}x${h}" -framerate "$rec_fps" -i "$tmp/frames.rgb" \
    -vf "fps=${fps},scale=${width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=${colors}:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
    -loop 0 "docs/gifs/$example.gif"

echo "docs/gifs/$example.gif ($(du -h "docs/gifs/$example.gif" | cut -f1))"
