#!/usr/bin/env bash
# End-to-end connection status lifecycle test between the PS Vita app (Vita3K) and syncthing-cli.
#
# Usage: scripts/vita3k_connection_lifecycle.sh [out_dir]
# Requires a VPK built with `CARGO_FEATURES=ui-script just vpk`.
#
# Validates connection status across the complete lifecycle (both Vita and CLI/server sides):
#   Phase 1 (Offline):      Vita starts while CLI is not running -> Offline state, no connection logged.
#                           Captures `1_offline_initial.png`.
#   Phase 2 (Connect+Sync): CLI starts -> Vita discovers, connects (1+2), syncs files both ways.
#                           Zero disconnects on either side.
#   Phase 3 (Live sync):    New file created on CLI while connected -> synced without dropping session.
#                           Captures `2_connected_synced.png` (green Ready / Connected 1+2).
#   Phase 4 (Disconnect):   CLI stops gracefully (SIGINT -> BEP Close) -> Vita logs clean session end,
#                           stays offline while CLI is stopped (no stale Connected state).
#                           Captures `3_disconnected_after_cli_stop.png` (Offline / Waiting for LAN).
#   Phase 5 (Reconnect +    CLI restarts -> Vita reconnects automatically and syncs a post-reconnect
#            Clean Quit):   file -> captures `4_reconnected_synced.png` -> Vita quits cleanly via
#                           START + CROSS -> CLI logs clean `Disconnected from vita: closed by peer: closing`.
set -euo pipefail

OUT_DIR="$(realpath -m "${1:-target/vita3k-connection-lifecycle}")"
UX0="${VITA3K_UX0:-$HOME/.local/share/Vita3K/Vita3K/ux0}"
DATA="$UX0/data/syncthing"
VITA_SHARE="$UX0/data/synctest-lifecycle"
FOLDER="vita-lifecycle-test"
VPK="target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk"

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
    rm -rf "$DATA" "$VITA_SHARE"
    [[ -d "$BACKUP/syncthing" ]] && cp -a "$BACKUP/syncthing" "$DATA"
    rm -rf "$BACKUP" "$WORK"
}
trap cleanup EXIT

id_to_hex() {
    python3 -c '
import base64, sys
chars = sys.argv[1].replace("-", "")
data = "".join(chars[i * 14:i * 14 + 13] for i in range(4))
print(base64.b32decode(data + "====").hex())
' "$1"
}

wait_file() {
    local path="$1" secs="$2" i
    for ((i = 0; i < secs * 4; i++)); do
        [[ -f "$path" ]] && return 0
        sleep 0.25
    done
    return 1
}

wait_grep() {
    local pattern="$1" file="$2" secs="$3" i
    for ((i = 0; i < secs * 4; i++)); do
        grep -qE "$pattern" "$file" 2>/dev/null && return 0
        sleep 0.25
    done
    return 1
}

free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}

CLI_PORT="$(free_port)"

start_cli_lan() {
    local log_file="$1"
    "$CLI" --home "$CLI_HOME" run --listen "0.0.0.0:$CLI_PORT" --no-global-discovery --no-relays \
        > "$log_file" 2>&1 &
    CLI_PID=$!
    wait_grep "Listening on 0\.0\.0\.0:$CLI_PORT" "$log_file" 10 || true
}

stop_cli_gracefully() {
    local log_file="$1"
    if [[ -n "$CLI_PID" ]]; then
        kill -INT "$CLI_PID" 2>/dev/null || true
        wait_grep "^\[.*\] Stopped$" "$log_file" 10 || true
        wait "$CLI_PID" 2>/dev/null || true
        CLI_PID=""
    fi
}

status=0
ok() { echo "OK   $1"; }
fail() { echo "FAIL $1"; status=1; }

# --- Identities & pairing -----------------------------------------------------------------
rm -rf "$DATA" "$VITA_SHARE"
mkdir -p "$DATA" "$VITA_SHARE" "$WORK/vita-id" "$WORK/cli-share/nested"
head -c 32 /dev/urandom > "$DATA/device_seed.bin"
cp "$DATA/device_seed.bin" "$WORK/vita-id/device_seed.bin"
VITA_ID="$("$CLI" --home "$WORK/vita-id" id)"
CLI_HOME="$WORK/cli"
CLI_ID="$("$CLI" --home "$CLI_HOME" id)"
CLI_HEX="$(id_to_hex "$CLI_ID")"
echo "Vita device: $VITA_ID"
echo "CLI device:  $CLI_ID"

"$CLI" --home "$CLI_HOME" config set name "linux-cli" > /dev/null
"$CLI" --home "$CLI_HOME" config set rescan-interval 2 > /dev/null
"$CLI" --home "$CLI_HOME" device add "$VITA_ID" --name "vita" > /dev/null
"$CLI" --home "$CLI_HOME" folder add "$FOLDER" "$WORK/cli-share" --device "$VITA_ID" > /dev/null
echo "written on CLI (phase 2)" > "$WORK/cli-share/from_cli.txt"
head -c 200000 /dev/urandom > "$WORK/cli-share/nested/blob.bin"

printf '%s|linux-cli|dynamic\n' "$CLI_HEX" > "$DATA/devices.cfg"
printf '%s|%s|ux0:data/synctest-lifecycle|1|%s|rw\n' "$FOLDER" "$FOLDER" "$CLI_HEX" > "$DATA/shares.cfg"
echo "written on Vita (phase 2)" > "$VITA_SHARE/from_vita.txt"

# Scripted timeline (60 VBlanks = ~1s):
#   5s  -> 1_offline_initial               (CLI not started yet)
#   30s -> 2_connected_synced              (CLI running, initial + live sync done, Ready/Connected 1+2)
#   42s -> 3_disconnected_after_cli_stop   (CLI stopped cleanly via SIGINT)
#   64s -> 4_reconnected_synced            (CLI restarted, reconnected + post_reconnect.txt synced)
#   65s -> START + CROSS                   (Vita quits cleanly -> sends BEP Close to CLI)
cat > "$DATA/ui_script.txt" <<'EOF'
wait 300
shot 1_offline_initial
wait 1500
shot 2_connected_synced
wait 720
shot 3_disconnected_after_cli_stop
wait 1320
shot 4_reconnected_synced
wait 30
press START
wait 30
press CROSS
EOF

mkdir -p "$UX0/app"
rm -rf "$DATA/shots"
unzip -qo "$VPK" -d "$UX0/app/SYNCTHING"

# --- Phase 1: Launch Vita while CLI is offline -------------------------------------------
echo "=== Phase 1: Offline initial state (CLI stopped) ==="
./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING > "$OUT_DIR/vita3k.log" 2>&1 &

if wait_file "$DATA/shots/1_offline_initial.rgba" 45; then
    ok "[phase1] Captured 1_offline_initial screenshot"
else
    fail "[phase1] Timed out waiting for 1_offline_initial.rgba"
fi

if ! grep -qE "Connected via TLS 1\.3|Connected \(" "$DATA/syncthing.log" 2>/dev/null; then
    ok "[phase1] Vita is offline before CLI starts (no connection logged)"
else
    fail "[phase1] Unexpected connection logged while CLI was stopped"
fi

# --- Phase 2 & 3: Start CLI -> connect, sync both ways, then live incremental sync -------
echo "=== Phase 2 & 3: Connection, bidirectional sync, and live update ==="
CLI_LOG_1="$OUT_DIR/cli_phase1.log"
start_cli_lan "$CLI_LOG_1"

if wait_grep "In sync with vita" "$CLI_LOG_1" 25; then
    ok "[phase2] CLI reached 'In sync with vita'"
else
    fail "[phase2] CLI did not reach 'In sync with vita'"
fi

# Trigger Phase 3 live update while session stays active.
echo "live delta while connected" > "$WORK/cli-share/live_update.txt"

if wait_file "$DATA/shots/2_connected_synced.rgba" 75; then
    ok "[phase3] Captured 2_connected_synced screenshot"
else
    fail "[phase3] Timed out waiting for 2_connected_synced.rgba"
fi

# Snapshot Vita log at end of Phase 3 (before stopping CLI).
cp "$DATA/syncthing.log" "$OUT_DIR/vita_phase3.log" 2>/dev/null || true

if cmp -s "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" &&
    cmp -s "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" &&
    cmp -s "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin"; then
    ok "[phase2] Initial bidirectional files match (from_vita.txt, from_cli.txt, nested/blob.bin)"
else
    fail "[phase2] Initial bidirectional file mismatch"
fi

if cmp -s "$WORK/cli-share/live_update.txt" "$VITA_SHARE/live_update.txt"; then
    ok "[phase3] Live update synced while connected (live_update.txt)"
else
    fail "[phase3] Live update live_update.txt did not sync"
fi

if grep -q "Connected via TLS 1\.3 to linux-cli" "$OUT_DIR/vita_phase3.log" 2>/dev/null &&
    grep -q "Connected (" "$OUT_DIR/vita_phase3.log" 2>/dev/null; then
    ok "[phase2-3] Vita logged active connected session"
else
    fail "[phase2-3] Vita missing Connected status in vita_phase3.log"
fi

if ! grep -q "ended (" "$OUT_DIR/vita_phase3.log" 2>/dev/null; then
    ok "[phase2-3] Zero spurious disconnects on Vita while CLI was online"
else
    fail "[phase2-3] Unexpected disconnect logged on Vita while CLI was online"
fi

if ! grep -q "Disconnected from vita" "$CLI_LOG_1" 2>/dev/null; then
    ok "[phase2-3] Zero spurious disconnects on CLI while Vita was online"
else
    fail "[phase2-3] Unexpected disconnect logged on CLI while Vita was online"
fi

# --- Phase 4: Stop CLI gracefully -> verify post-disconnection state on Vita -------------
echo "=== Phase 4: Graceful CLI disconnect & Vita post-disconnection state ==="
stop_cli_gracefully "$CLI_LOG_1"

if grep -q "Stopped" "$CLI_LOG_1" 2>/dev/null; then
    ok "[phase4] CLI stopped cleanly"
else
    fail "[phase4] CLI did not log clean stop"
fi

if wait_file "$DATA/shots/3_disconnected_after_cli_stop.rgba" 50; then
    ok "[phase4] Captured 3_disconnected_after_cli_stop screenshot"
else
    fail "[phase4] Timed out waiting for 3_disconnected_after_cli_stop.rgba"
fi

cp "$DATA/syncthing.log" "$OUT_DIR/vita_phase4.log" 2>/dev/null || true

if grep -qE "Session with .* ended \(closed by peer: closing\)\. Reconnecting\.\.\." "$OUT_DIR/vita_phase4.log" 2>/dev/null; then
    ok "[phase4] Vita logged clean BEP Close disconnection"
else
    fail "[phase4] Vita did not log clean BEP Close disconnection"
fi

# After the disconnect line, Vita must remain offline (no new Connected entry) while CLI is stopped.
AFTER_DISC="$(awk '/ended \(closed by peer: closing\)/{found=1; next} found{print}' "$OUT_DIR/vita_phase4.log" 2>/dev/null || true)"
if [[ -z "$(echo "$AFTER_DISC" | grep -E "Connected via TLS 1\.3|Connected \(" || true)" ]]; then
    ok "[phase4] Post-disconnection state stayed offline while CLI was stopped"
else
    fail "[phase4] Unexpected Connected log after CLI stopped"
fi

# --- Phase 5: Restart CLI -> reconnect + sync -> Vita clean quit -> CLI disconnect state -
echo "=== Phase 5: Reconnection, post-reconnect sync, and clean Vita quit ==="
echo "written after reconnect (phase 5)" > "$WORK/cli-share/post_reconnect.txt"
CLI_LOG_2="$OUT_DIR/cli_phase2.log"
start_cli_lan "$CLI_LOG_2"

if wait_file "$DATA/shots/4_reconnected_synced.rgba" 75; then
    ok "[phase5] Captured 4_reconnected_synced screenshot"
else
    fail "[phase5] Timed out waiting for 4_reconnected_synced.rgba"
fi

# Wait for Vita's scripted START+CROSS quit to send BEP Close to CLI.
if wait_grep "Disconnected from vita: closed by peer: closing" "$CLI_LOG_2" 25; then
    ok "[phase5] CLI observed clean post-disconnection state (closed by peer: closing)"
else
    fail "[phase5] CLI did not observe clean BEP Close after Vita quit"
fi

sleep 1
pkill -x Vita3K 2>/dev/null || true
stop_cli_gracefully "$CLI_LOG_2"

cp "$DATA/syncthing.log" "$OUT_DIR/vita.log" 2>/dev/null || true
cp "$DATA/devices.cfg" "$OUT_DIR/devices.cfg" 2>/dev/null || true

if cmp -s "$WORK/cli-share/post_reconnect.txt" "$VITA_SHARE/post_reconnect.txt"; then
    ok "[phase5] Post-reconnect file synced (post_reconnect.txt)"
else
    fail "[phase5] Post-reconnect file post_reconnect.txt did not sync"
fi

if grep -q "Connected to vita" "$CLI_LOG_2" 2>/dev/null &&
    grep -q "In sync with vita" "$CLI_LOG_2" 2>/dev/null; then
    ok "[phase5] CLI logged reconnect and 'In sync with vita'"
else
    fail "[phase5] CLI missing reconnect / In sync logs in cli_phase2.log"
fi

if grep -q "Exit requested by user \[START\]\." "$OUT_DIR/vita.log" 2>/dev/null; then
    ok "[phase5] Vita performed clean user shutdown"
else
    fail "[phase5] Vita did not log clean user shutdown"
fi

# Convert captured RGBA frames to PNG.
for f in "$DATA"/shots/*.rgba; do
    [[ -f "$f" ]] || continue
    png="$OUT_DIR/$(basename "$f" .rgba).png"
    python3 -c "
import sys
from PIL import Image
Image.frombytes('RGBA', (960, 544), open(sys.argv[1], 'rb').read()).convert('RGB').save(sys.argv[2])
" "$f" "$png"
done
rm -f "$DATA/ui_script.txt"

echo "Logs and screenshots: $OUT_DIR"
exit "$status"
