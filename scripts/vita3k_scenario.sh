#!/usr/bin/env bash
# Replays a UI input script in Vita3K and converts the in-app frame dumps to PNG.
#
# Usage: scripts/vita3k_scenario.sh <script.txt> [run_seconds] [out_dir]
# Requires a VPK built with `CARGO_FEATURES=ui-script just vpk`.
# Script steps (one per line): `wait <frames>`, `press <BUTTON>`, `tap <x> <y>`,
# `ime <text>`, `shot <name>` — see crates/vita-syncthing-app/src/infrastructure/ui_script.rs.
# The app data directory (ux0:data/syncthing) is backed up and restored around the run.
# Works without Sony firmware and without a display (see scripts/vita3k_launch.sh).
set -euo pipefail

SCRIPT="$(realpath "$1")"
RUN_SECS="${2:-15}"
OUT_DIR="$(realpath -m "${3:-target/ui-shots}")"
UX0="${VITA3K_UX0:-$HOME/.local/share/Vita3K/Vita3K/ux0}"
DATA="$UX0/data/syncthing"
VPK="target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk"

cd "$(dirname "$0")/.."
BACKUP="$(mktemp -d)"
[[ -d "$DATA" ]] && cp -a "$DATA" "$BACKUP/syncthing"
restore() {
    pkill -x Vita3K 2>/dev/null || true
    rm -rf "$DATA"
    [[ -d "$BACKUP/syncthing" ]] && cp -a "$BACKUP/syncthing" "$DATA"
    rm -rf "$BACKUP"
}
trap restore EXIT

mkdir -p "$DATA" "$OUT_DIR" "$UX0/app"
rm -rf "$DATA/shots"
cp "$SCRIPT" "$DATA/ui_script.txt"
unzip -qo "$VPK" -d "$UX0/app/SYNCTHING"

./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING > "$OUT_DIR/vita3k.log" 2>&1 &
sleep "$RUN_SECS"
pkill -x Vita3K || true
sleep 1

for f in "$DATA"/shots/*.rgba; do
    [[ -f "$f" ]] || continue
    png="$OUT_DIR/$(basename "$f" .rgba).png"
    python3 -c "
import sys
from PIL import Image
Image.frombytes('RGBA', (960, 544), open(sys.argv[1], 'rb').read()).convert('RGB').save(sys.argv[2])
" "$f" "$png"
    echo "$png"
done
