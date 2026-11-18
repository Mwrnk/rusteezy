#!/usr/bin/env bash
# Regenerate every README GIF in docs/gifs/. Needs ffmpeg.
# Grid GIFs are 480 px wide (the README shows them two per row); heroes are 640.
set -euo pipefail
cd "$(dirname "$0")"

#                          example    start seconds  extra args
GIF_WIDTH=640              ./gif.sh crab       0     6
GIF_WIDTH=640              ./gif.sh tree       0     9   --font 7
GIF_WIDTH=480              ./gif.sh fireworks  2     6
GIF_WIDTH=480              ./gif.sh fire       2     3.5
GIF_WIDTH=440 GIF_FPS=12   ./gif.sh plasma     0     4
GIF_WIDTH=480              ./gif.sh torus      0     5
GIF_WIDTH=480 GIF_FPS=12   ./gif.sh starfield  3     4
GIF_WIDTH=480              ./gif.sh galaxy     0     6
GIF_WIDTH=480              ./gif.sh flow       4     5
GIF_WIDTH=480              ./gif.sh raymarch   0     6
GIF_WIDTH=440 GIF_FPS=12   ./gif.sh life       1     3   --font 6
GIF_WIDTH=640              ./gif.sh bridge     0     9.5
