#!/usr/bin/env bash
# End-to-end sync test between the PS Vita app (in Vita3K) and syncthing-cli on this computer.
#
# Usage: scripts/vita3k_cli_sync.sh [run_seconds] [out_dir]
# Requires a VPK built with `CARGO_FEATURES=ui-script just vpk` (screenshots of the result).
#
# 1. Generates the Vita's identity seed, so its device ID is known in advance (the CLI derives
#    the same ID from the same seed), and pairs both sides by writing their configuration:
#    the Vita knows the CLI with a `dynamic` address (found by LAN discovery), the CLI knows
#    the Vita (which never listens: the Vita always dials, the CLI accepts).
# 2. Shares the folder `vita-cli-test`: `ux0:data/synctest` on the Vita, a temp dir on the CLI,
#    each side starting with a file of its own.
# 3. Runs the CLI daemon, then the Vita app in Vita3K, and checks that both sides end with
#    both files. The Vita's protocol log, the CLI log and the screenshots land in `out_dir`.
set -euo pipefail

RUN_SECS="${1:-75}"
OUT_DIR="$(realpath -m "${2:-target/vita3k-cli-sync}")"
UX0="${VITA3K_UX0:-$HOME/.local/share/Vita3K/Vita3K/ux0}"
DATA="$UX0/data/syncthing"
VITA_SHARE="$UX0/data/synctest"
FOLDER="vita-cli-test"

cd "$(dirname "$0")/.."
cargo build -q -p syncthing-cli
CLI="$PWD/target/debug/syncthing-cli"
WORK="$(mktemp -d)"
rm -rf "$OUT_DIR" && mkdir -p "$OUT_DIR"

BACKUP="$(mktemp -d)"
[[ -d "$DATA" ]] && cp -a "$DATA" "$BACKUP/syncthing"
CLI_PID=""
cleanup() {
    [[ -n "$CLI_PID" ]] && kill "$CLI_PID" 2>/dev/null || true
    pkill -x Vita3K 2>/dev/null || true
    rm -rf "$DATA"
    [[ -d "$BACKUP/syncthing" ]] && cp -a "$BACKUP/syncthing" "$DATA"
    rm -rf "$BACKUP" "$WORK"
}
trap cleanup EXIT

# Device ID (Syncthing form) -> 64-char hex (`devices.cfg` / `shares.cfg` format).
id_to_hex() {
    python3 -c '
import base64, sys
chars = sys.argv[1].replace("-", "")
data = "".join(chars[i * 14:i * 14 + 13] for i in range(4))  # drop the Luhn check characters
print(base64.b32decode(data + "====").hex())
' "$1"
}

# --- Identities -------------------------------------------------------------------------
rm -rf "$DATA" "$VITA_SHARE"
mkdir -p "$DATA" "$VITA_SHARE" "$WORK/vita-id"
head -c 32 /dev/urandom > "$DATA/device_seed.bin"
cp "$DATA/device_seed.bin" "$WORK/vita-id/device_seed.bin"
VITA_ID="$("$CLI" --home "$WORK/vita-id" id)"
CLI_HOME="$WORK/cli"
CLI_ID="$("$CLI" --home "$CLI_HOME" id)"
echo "Vita device: $VITA_ID"
echo "CLI device:  $CLI_ID"

# --- CLI side -----------------------------------------------------------------------------
"$CLI" --home "$CLI_HOME" config set name "linux-cli" > /dev/null
"$CLI" --home "$CLI_HOME" config set rescan-interval 2 > /dev/null
"$CLI" --home "$CLI_HOME" device add "$VITA_ID" --name "vita" > /dev/null
"$CLI" --home "$CLI_HOME" folder add "$FOLDER" "$WORK/cli-share" --device "$VITA_ID" > /dev/null
echo "written on the CLI" > "$WORK/cli-share/from_cli.txt"
mkdir -p "$WORK/cli-share/nested"
head -c 300000 /dev/urandom > "$WORK/cli-share/nested/blob.bin"

# --- Vita side ----------------------------------------------------------------------------
CLI_HEX="$(id_to_hex "$CLI_ID")"
printf '%s|linux-cli|dynamic\n' "$CLI_HEX" > "$DATA/devices.cfg"
printf '%s|%s|ux0:data/synctest|1|%s|rw\n' "$FOLDER" "$FOLDER" "$CLI_HEX" > "$DATA/shares.cfg"
echo "written on the Vita" > "$VITA_SHARE/from_vita.txt"

# --- Run ----------------------------------------------------------------------------------
"$CLI" --home "$CLI_HOME" run --listen 0.0.0.0:22000 --no-global-discovery --no-relays \
    > "$OUT_DIR/cli.log" 2>&1 &
CLI_PID=$!
sleep 1

# Overview once synced, then Settings (protocol log card).
SCRIPT="$WORK/ui_script.txt"
FRAMES=$(( (RUN_SECS - 15) * 60 ))
printf 'wait %d\nshot 1_overview\npress R\nwait 20\npress R\nwait 30\nshot 2_settings\n' \
    "$FRAMES" > "$SCRIPT"
# Copies the Vita's log while it runs (the scenario restores the data directory at the end).
( while sleep 2; do cp "$DATA/syncthing.log" "$OUT_DIR/vita.log" 2>/dev/null || true; done ) &
COPIER=$!
./scripts/vita3k_scenario.sh "$SCRIPT" "$RUN_SECS" "$OUT_DIR" || true
kill "$COPIER" 2>/dev/null || true
kill "$CLI_PID" 2>/dev/null || true
wait "$CLI_PID" 2>/dev/null || true
CLI_PID=""

# --- Check --------------------------------------------------------------------------------
status=0
check() {
    if cmp -s "$1" "$2"; then echo "OK   $3"; else echo "FAIL $3"; status=1; fi
}
check "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" "Vita -> CLI: from_vita.txt"
check "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" "CLI -> Vita: from_cli.txt"
check "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin" "CLI -> Vita: nested/blob.bin"
echo "Logs and screenshots: $OUT_DIR"
exit $status
