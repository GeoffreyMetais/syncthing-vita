#!/usr/bin/env bash
# End-to-end sync test between the PS Vita app (in Vita3K) and syncthing-cli on this computer.
#
# Usage: scripts/vita3k_cli_sync.sh [run_seconds] [out_dir] [lan|internet|internet-then-lan|lan-then-internet]
# Requires a VPK built with `CARGO_FEATURES=ui-script just vpk`.
#
# Modes:
#   lan               (default) CLI listens on 0.0.0.0:22000; Vita finds it via UDP LAN discovery.
#   internet          CLI disables local discovery and listens on no TCP port; it joins a public
#                     relay and announces to Global Discovery, so Vita must dial it over Internet.
#   internet-then-lan Phase 1 syncs over Internet/relay, verifies devices.cfg stays `dynamic`,
#                     then Phase 2 keeps the same Vita DATA dir and restarts CLI on LAN, verifying
#                     new files sync over a local LAN IP (and not a relay address).
#   lan-then-internet Phase 1 syncs over LAN (saving a LAN IP in devices.cfg), then Phase 2 stops
#                     the LAN listener and switches CLI to Internet/relay-only, verifying Vita falls
#                     back to Global Discovery + relay instead of getting stuck on the stale LAN IP.
#
# 1. Generates the Vita's identity seed, so its device ID is known in advance (the CLI derives
#    the same ID from the same seed), and pairs both sides by writing their configuration:
#    the Vita knows the CLI with a `dynamic` address, the CLI knows the Vita (which never listens).
# 2. Shares the folder `vita-cli-test`: `ux0:data/synctest` on the Vita, a temp dir on the CLI,
#    each side starting with a file of its own.
# 3. Runs the CLI daemon, then the Vita app in Vita3K, and checks that both sides end with
#    both files. The Vita's protocol log, the CLI log and the screenshots land in `out_dir`.
set -euo pipefail

RUN_SECS="${1:-75}"
OUT_DIR="$(realpath -m "${2:-target/vita3k-cli-sync}")"
MODE="${3:-lan}"
if [[ "$MODE" != "lan" && "$MODE" != "internet" && "$MODE" != "internet-then-lan" && "$MODE" != "lan-then-internet" ]]; then
    echo "Usage: $0 [run_seconds] [out_dir] [lan|internet|internet-then-lan|lan-then-internet]" >&2
    exit 2
fi
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
COPIER=""
cleanup() {
    [[ -n "$COPIER" ]] && kill "$COPIER" 2>/dev/null || true
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

start_cli_lan() {
    local log_file="$1"
    "$CLI" --home "$CLI_HOME" run --listen 0.0.0.0:22000 --no-global-discovery --no-relays \
        > "$log_file" 2>&1 &
    CLI_PID=$!
    sleep 1
}

start_cli_internet() {
    local log_file="$1"
    mkdir -p "$(dirname "$log_file")"
    "$CLI" --home "$CLI_HOME" run --listen none --no-local-discovery \
        > "$log_file" 2>&1 &
    CLI_PID=$!
    for _ in $(seq 1 180); do
        if grep -qE "Global Discovery: announced .*relay://" "$log_file" 2>/dev/null; then
            break
        fi
        sleep 0.5
    done
    if ! grep -qE "Global Discovery: announced .*relay://" "$log_file" 2>/dev/null; then
        echo "FAIL CLI did not announce a relay URI to Global Discovery within 90s"
        cat "$log_file" || true
        exit 1
    fi
}

# Starts CLI with LAN discovery, TCP 22000, Global Discovery, and Relays ALL enabled so that
# Phase 2 genuinely proves the Vita prefers LAN even when Global Discovery / relays are available.
start_cli_lan_and_global() {
    local log_file="$1"
    mkdir -p "$(dirname "$log_file")"
    "$CLI" --home "$CLI_HOME" run --listen 0.0.0.0:22000 \
        > "$log_file" 2>&1 &
    CLI_PID=$!
    for _ in $(seq 1 180); do
        if grep -qE "Global Discovery: announced .*relay://" "$log_file" 2>/dev/null; then
            break
        fi
        sleep 0.5
    done
    if ! grep -qE "Global Discovery: announced .*relay://" "$log_file" 2>/dev/null; then
        echo "FAIL CLI did not announce to Global Discovery in LAN+Global mode within 90s"
        cat "$log_file" || true
        exit 1
    fi
}

stop_cli() {
    if [[ -n "$CLI_PID" ]]; then
        kill "$CLI_PID" 2>/dev/null || true
        wait "$CLI_PID" 2>/dev/null || true
        CLI_PID=""
    fi
}

# Runs Vita3K directly (same launch wrapper as vita3k_scenario.sh) so `$DATA` persists cleanly
# across phases instead of being rolled back by `vita3k_scenario.sh`'s EXIT trap.
run_vita3k_phase() {
    local phase_out="$1"
    local run_secs="$2"
    mkdir -p "$phase_out" "$DATA" "$UX0/app"
    local script="$WORK/ui_script.txt"
    local frames=$(( (run_secs > 15 ? run_secs - 15 : 5) * 60 ))
    printf 'wait %d\nshot 1_overview\npress R\nwait 20\npress R\nwait 30\nshot 2_settings\n' \
        "$frames" > "$script"

    # Copies the Vita's log while it runs so diagnostics survive even if Vita3K is stopped mid-flush.
    ( while sleep 2; do cp "$DATA/syncthing.log" "$phase_out/vita.log" 2>/dev/null || true; done ) &
    COPIER=$!

    local vpk="target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk"
    rm -rf "$DATA/shots"
    cp "$script" "$DATA/ui_script.txt"
    unzip -qo "$vpk" -d "$UX0/app/SYNCTHING"

    ./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING > "$phase_out/vita3k.log" 2>&1 &
    sleep "$run_secs"
    pkill -x Vita3K || true
    sleep 1

    kill "$COPIER" 2>/dev/null || true
    wait "$COPIER" 2>/dev/null || true
    COPIER=""

    cp "$DATA/syncthing.log" "$phase_out/vita.log" 2>/dev/null || true
    cp "$DATA/devices.cfg" "$phase_out/devices.cfg" 2>/dev/null || true
    for f in "$DATA"/shots/*.rgba; do
        [[ -f "$f" ]] || continue
        local png="$phase_out/$(basename "$f" .rgba).png"
        python3 -c "
import sys
from PIL import Image
Image.frombytes('RGBA', (960, 544), open(sys.argv[1], 'rb').read()).convert('RGB').save(sys.argv[2])
" "$f" "$png"
    done
    rm -f "$DATA/ui_script.txt"
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

status=0
check() {
    if cmp -s "$1" "$2"; then echo "OK   $3"; else echo "FAIL $3"; status=1; fi
}

# --- Run ----------------------------------------------------------------------------------
if [[ "$MODE" == "lan" ]]; then
    start_cli_lan "$OUT_DIR/cli.log"
    run_vita3k_phase "$OUT_DIR" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" "Vita -> CLI: from_vita.txt"
    check "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" "CLI -> Vita: from_cli.txt"
    check "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin" "CLI -> Vita: nested/blob.bin"
elif [[ "$MODE" == "internet" ]]; then
    start_cli_internet "$OUT_DIR/cli.log"
    run_vita3k_phase "$OUT_DIR" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" "Vita -> CLI: from_vita.txt"
    check "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" "CLI -> Vita: from_cli.txt"
    check "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin" "CLI -> Vita: nested/blob.bin"
elif [[ "$MODE" == "internet-then-lan" ]]; then
    # Phase 1: Internet / relay sync
    echo "=== Phase 1: Internet / relay sync ==="
    PHASE1_OUT="$OUT_DIR/phase1-internet"
    start_cli_internet "$PHASE1_OUT/cli.log"
    run_vita3k_phase "$PHASE1_OUT" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" "Phase 1 Vita -> CLI: from_vita.txt"
    check "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" "Phase 1 CLI -> Vita: from_cli.txt"
    check "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin" "Phase 1 CLI -> Vita: nested/blob.bin"

    if grep -qE "^${CLI_HEX}\|.*\|dynamic$" "$DATA/devices.cfg" 2>/dev/null; then
        echo "OK   Phase 1 devices.cfg kept 'dynamic' address"
    else
        echo "FAIL Phase 1 devices.cfg did not keep 'dynamic' address:"
        cat "$DATA/devices.cfg" 2>/dev/null || true
        status=1
    fi

    # Phase 2: LAN + Global + Relay all enabled, reusing the exact same Vita DATA dir
    # (preserving devices.cfg, shares.cfg, device_seed.bin, sync/) and a fresh Vita log so Phase 2
    # TLS connections unambiguously prove LAN preference even when Global/Relay are available.
    echo "=== Phase 2: LAN + Global reconnection sync ==="
    PHASE2_OUT="$OUT_DIR/phase2-lan"
    rm -f "$DATA/syncthing.log" "$DATA/syncthing.log.1"
    echo "written on the CLI (LAN phase)" > "$WORK/cli-share/from_cli_lan.txt"
    echo "written on the Vita (LAN phase)" > "$VITA_SHARE/from_vita_lan.txt"

    start_cli_lan_and_global "$PHASE2_OUT/cli.log"
    run_vita3k_phase "$PHASE2_OUT" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita_lan.txt" "$WORK/cli-share/from_vita_lan.txt" "Phase 2 Vita -> CLI: from_vita_lan.txt"
    check "$WORK/cli-share/from_cli_lan.txt" "$VITA_SHARE/from_cli_lan.txt" "Phase 2 CLI -> Vita: from_cli_lan.txt"

    if grep -qE "Connected via TLS 1\.3 to .* \((127\.|10\.|192\.168\.|172\.(1[6-9]|2[0-9]|3[0-1])\.)" "$PHASE2_OUT/vita.log" 2>/dev/null; then
        echo "OK   Phase 2 connected over local/LAN IP"
    else
        echo "FAIL Phase 2 did not log a TLS 1.3 connection to a local/LAN IP"
        grep -E "Connected via TLS 1\.3" "$PHASE2_OUT/vita.log" 2>/dev/null || true
        status=1
    fi
    if grep -qE "Connected via TLS 1\.3 to .*relay://" "$PHASE2_OUT/vita.log" 2>/dev/null; then
        echo "FAIL Phase 2 unexpectedly connected via a relay address"
        status=1
    fi
else
    # Phase 1: LAN sync (records a LAN IP in devices.cfg)
    echo "=== Phase 1: LAN sync ==="
    PHASE1_OUT="$OUT_DIR/phase1-lan"
    start_cli_lan "$PHASE1_OUT/cli.log"
    run_vita3k_phase "$PHASE1_OUT" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita.txt" "$WORK/cli-share/from_vita.txt" "Phase 1 Vita -> CLI: from_vita.txt"
    check "$WORK/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt" "Phase 1 CLI -> Vita: from_cli.txt"
    check "$WORK/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin" "Phase 1 CLI -> Vita: nested/blob.bin"

    # Phase 2: Leave the LAN — CLI switches to Internet/relay-only while Vita still has a stale
    # LAN IP in devices.cfg. Vita must not get stuck on `Peer unreachable (waiting for LAN)`.
    echo "=== Phase 2: Off-LAN Global Discovery + relay fallback ==="
    PHASE2_OUT="$OUT_DIR/phase2-internet"
    rm -f "$DATA/syncthing.log" "$DATA/syncthing.log.1"
    echo "written on the CLI (off-LAN phase)" > "$WORK/cli-share/from_cli_wan.txt"
    echo "written on the Vita (off-LAN phase)" > "$VITA_SHARE/from_vita_wan.txt"

    start_cli_internet "$PHASE2_OUT/cli.log"
    run_vita3k_phase "$PHASE2_OUT" "$RUN_SECS"
    stop_cli

    check "$VITA_SHARE/from_vita_wan.txt" "$WORK/cli-share/from_vita_wan.txt" "Phase 2 Vita -> CLI: from_vita_wan.txt"
    check "$WORK/cli-share/from_cli_wan.txt" "$VITA_SHARE/from_cli_wan.txt" "Phase 2 CLI -> Vita: from_cli_wan.txt"

    if grep -qE "Connected via TLS 1\.3 to .*relay://" "$PHASE2_OUT/vita.log" 2>/dev/null; then
        echo "OK   Phase 2 fell back to relay via Global Discovery"
    else
        echo "FAIL Phase 2 did not log a TLS 1.3 connection via relay"
        grep -E "Connected via TLS 1\.3" "$PHASE2_OUT/vita.log" 2>/dev/null || true
        status=1
    fi
fi

echo "Logs and screenshots: $OUT_DIR"
exit $status
