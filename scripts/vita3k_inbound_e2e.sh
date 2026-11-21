#!/usr/bin/env bash
# End-to-end tests for PS Vita inbound reachability and the DeviceOffer modal (Vita3K <-> syncthing-cli).
#
# Usage: scripts/vita3k_inbound_e2e.sh [all|sync|accept|decline] [out_dir]
# Requires a VPK built with `CARGO_FEATURES=ui-script just vpk`.
#
# Scenarios (in every case syncthing-cli runs with `--listen none --no-local-discovery` so ONLY
# the CLI can initiate the TCP connection to the PS Vita on 127.0.0.1:22000):
#   sync     Paired CLI dials the Vita inbound and syncs files in both directions.
#   accept   Vita starts with 0 paired devices; unknown CLI dials Vita -> DeviceOffer modal ->
#            Cross accepts -> devices.cfg persisted with `dynamic` -> CLI reconnects & offers folder.
#   decline  Vita starts with 0 paired devices; unknown CLI dials Vita -> DeviceOffer modal ->
#            Square declines -> ignored_devices.cfg persisted, devices.cfg stays empty.
set -euo pipefail

MODE="${1:-all}"
OUT_DIR="$(realpath -m "${2:-target/vita3k-inbound-e2e}")"
if [[ "$MODE" != "all" && "$MODE" != "sync" && "$MODE" != "accept" && "$MODE" != "decline" ]]; then
    echo "Usage: $0 [all|sync|accept|decline] [out_dir]" >&2
    exit 2
fi

UX0="${VITA3K_UX0:-$HOME/.local/share/Vita3K/Vita3K/ux0}"
DATA="$UX0/data/syncthing"
VITA_SHARE="$UX0/data/synctest-in"
VPK="target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk"

cd "$(dirname "$0")/.."
cargo build -q -p syncthing-cli
CLI="$PWD/target/debug/syncthing-cli"
rm -rf "$OUT_DIR" && mkdir -p "$OUT_DIR"

BACKUP="$(mktemp -d)"
[[ -d "$DATA" ]] && cp -a "$DATA" "$BACKUP/syncthing"
CLI_PID=""
cleanup() {
    [[ -n "$CLI_PID" ]] && kill "$CLI_PID" 2>/dev/null || true
    pkill -x Vita3K 2>/dev/null || true
    rm -rf "$DATA" "$VITA_SHARE"
    [[ -d "$BACKUP/syncthing" ]] && cp -a "$BACKUP/syncthing" "$DATA"
    rm -rf "$BACKUP"
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

dump_shots() {
    local dest="$1"
    for f in "$DATA"/shots/*.rgba; do
        [[ -f "$f" ]] || continue
        local png="$dest/$(basename "$f" .rgba).png"
        python3 -c '
import sys
from PIL import Image
Image.frombytes("RGBA", (960, 544), open(sys.argv[1], "rb").read()).convert("RGB").save(sys.argv[2])
' "$f" "$png"
    done
}

run_vita3k_with_cli() {
    local sub_out="$1" cli_home="$2" run_secs="$3"
    mkdir -p "$sub_out" "$UX0/app"
    rm -rf "$DATA/shots"
    unzip -qo "$VPK" -d "$UX0/app/SYNCTHING"

    ./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING > "$sub_out/vita3k.log" 2>&1 &
    sleep 3

    "$CLI" --home "$cli_home" run \
        --listen none --no-local-discovery --no-global-discovery --no-relays \
        > "$sub_out/cli.log" 2>&1 &
    CLI_PID=$!

    sleep "$run_secs"
    pkill -x Vita3K || true
    kill "$CLI_PID" 2>/dev/null || true
    wait "$CLI_PID" 2>/dev/null || true
    CLI_PID=""
    sleep 1

    cp "$DATA/syncthing.log" "$sub_out/vita.log" 2>/dev/null || true
    cp "$DATA/devices.cfg" "$sub_out/devices.cfg" 2>/dev/null || true
    cp "$DATA/ignored_devices.cfg" "$sub_out/ignored_devices.cfg" 2>/dev/null || true
    dump_shots "$sub_out"
}

TOTAL_STATUS=0

# --- Scenario 1: Inbound bidirectional sync (already-paired CLI dials Vita) ---------------
run_sync() {
    echo "=== Scenario 1: Inbound bidirectional sync ==="
    local work sub="$OUT_DIR/sync"
    work="$(mktemp -d)"
    rm -rf "$DATA" "$VITA_SHARE"
    mkdir -p "$DATA" "$VITA_SHARE" "$work/vita-id" "$work/cli-share/nested"

    head -c 32 /dev/urandom > "$DATA/device_seed.bin"
    cp "$DATA/device_seed.bin" "$work/vita-id/device_seed.bin"
    local vita_id cli_id cli_hex folder="vita-inbound-test"
    vita_id="$("$CLI" --home "$work/vita-id" id)"
    cli_id="$("$CLI" --home "$work/cli" id)"
    cli_hex="$(id_to_hex "$cli_id")"

    "$CLI" --home "$work/cli" config set name "linux-cli" > /dev/null
    "$CLI" --home "$work/cli" config set rescan-interval 2 > /dev/null
    "$CLI" --home "$work/cli" device add "$vita_id" --name "vita" --address "127.0.0.1:22000" > /dev/null
    "$CLI" --home "$work/cli" folder add "$folder" "$work/cli-share" --device "$vita_id" > /dev/null
    echo "written on the CLI (inbound)" > "$work/cli-share/from_cli.txt"
    head -c 160000 /dev/urandom > "$work/cli-share/nested/blob.bin"

    # Vita knows CLI with `dynamic` address and no LAN discovery from CLI -> only CLI can dial Vita.
    printf '%s|linux-cli|dynamic\n' "$cli_hex" > "$DATA/devices.cfg"
    printf '%s|%s|ux0:data/synctest-in|1|%s|rw\n' "$folder" "$folder" "$cli_hex" > "$DATA/shares.cfg"
    echo "written on the Vita (inbound)" > "$VITA_SHARE/from_vita.txt"
    printf 'wait 1200\nshot overview_synced\n' > "$DATA/ui_script.txt"

    run_vita3k_with_cli "$sub" "$work/cli" 26

    local st=0
    if grep -q "Listening for incoming BEP/1.0 connections on 0.0.0.0:22000" "$sub/vita.log" 2>/dev/null; then
        echo "OK   [sync] Vita listening on 0.0.0.0:22000"
    else
        echo "FAIL [sync] Vita did not log TCP 22000 listener"
        st=1
    fi
    if cmp -s "$VITA_SHARE/from_vita.txt" "$work/cli-share/from_vita.txt"; then
        echo "OK   [sync] Vita -> CLI: from_vita.txt"
    else
        echo "FAIL [sync] Vita -> CLI: from_vita.txt"
        st=1
    fi
    if cmp -s "$work/cli-share/from_cli.txt" "$VITA_SHARE/from_cli.txt"; then
        echo "OK   [sync] CLI -> Vita: from_cli.txt"
    else
        echo "FAIL [sync] CLI -> Vita: from_cli.txt"
        st=1
    fi
    if cmp -s "$work/cli-share/nested/blob.bin" "$VITA_SHARE/nested/blob.bin"; then
        echo "OK   [sync] CLI -> Vita: nested/blob.bin"
    else
        echo "FAIL [sync] CLI -> Vita: nested/blob.bin"
        st=1
    fi
    rm -rf "$work"
    [[ "$st" -ne 0 ]] && TOTAL_STATUS=1
    return 0
}

# --- Scenario 2: Unknown peer -> DeviceOffer -> Cross (Accept) -> FolderOffer -------------
run_accept() {
    echo "=== Scenario 2: DeviceOffer accept (0 paired devices at launch) ==="
    local work sub="$OUT_DIR/accept"
    work="$(mktemp -d)"
    rm -rf "$DATA"
    mkdir -p "$DATA" "$work/vita-id" "$work/cli-share"

    head -c 32 /dev/urandom > "$DATA/device_seed.bin"
    cp "$DATA/device_seed.bin" "$work/vita-id/device_seed.bin"
    local vita_id cli_id cli_hex
    vita_id="$("$CLI" --home "$work/vita-id" id)"
    cli_id="$("$CLI" --home "$work/cli" id)"
    cli_hex="$(id_to_hex "$cli_id")"

    "$CLI" --home "$work/cli" config set name "linux-cli" > /dev/null
    "$CLI" --home "$work/cli" device add "$vita_id" --name "vita" --address "127.0.0.1:22000" > /dev/null
    echo "offered file" > "$work/cli-share/hello.txt"
    "$CLI" --home "$work/cli" folder add "inbound-offer" "$work/cli-share" --device "$vita_id" > /dev/null

    # Wait 7s for DeviceOffer modal -> shot -> press CROSS -> wait 34s for CLI's 30s RETRY_AWAITING_PAIRING reconnect + FolderOffer -> shot
    printf 'wait 420\nshot 1_device_offer\npress CROSS\nwait 2040\nshot 2_after_accept\n' > "$DATA/ui_script.txt"

    run_vita3k_with_cli "$sub" "$work/cli" 46

    local st=0
    if grep -q "wants to connect: waiting for approval" "$sub/vita.log" 2>/dev/null; then
        echo "OK   [accept] Unknown device offer queued"
    else
        echo "FAIL [accept] No DeviceOffer logged"
        st=1
    fi
    if grep -q "Accepted incoming device" "$sub/vita.log" 2>/dev/null; then
        echo "OK   [accept] Device accepted via Cross"
    else
        echo "FAIL [accept] Device acceptance not logged"
        st=1
    fi
    if grep -q "^${cli_hex}|.*|dynamic$" "$sub/devices.cfg" 2>/dev/null; then
        echo "OK   [accept] devices.cfg persisted with dynamic address"
    else
        echo "FAIL [accept] devices.cfg missing expected dynamic entry"
        st=1
    fi
    if grep -q "wants to share 'inbound-offer'" "$sub/vita.log" 2>/dev/null; then
        echo "OK   [accept] CLI reconnected and FolderOffer received"
    else
        echo "FAIL [accept] FolderOffer not received after accepting device"
        st=1
    fi
    rm -rf "$work"
    [[ "$st" -ne 0 ]] && TOTAL_STATUS=1
    return 0
}

# --- Scenario 3: Unknown peer -> DeviceOffer -> Square (Decline) -> ignored_devices.cfg ---
run_decline() {
    echo "=== Scenario 3: DeviceOffer decline (ignored_devices.cfg) ==="
    local work sub="$OUT_DIR/decline"
    work="$(mktemp -d)"
    rm -rf "$DATA"
    mkdir -p "$DATA" "$work/vita-id"

    head -c 32 /dev/urandom > "$DATA/device_seed.bin"
    cp "$DATA/device_seed.bin" "$work/vita-id/device_seed.bin"
    local vita_id cli_id cli_hex
    vita_id="$("$CLI" --home "$work/vita-id" id)"
    cli_id="$("$CLI" --home "$work/cli" id)"
    cli_hex="$(id_to_hex "$cli_id")"

    "$CLI" --home "$work/cli" config set name "linux-cli" > /dev/null
    "$CLI" --home "$work/cli" device add "$vita_id" --name "vita" --address "127.0.0.1:22000" > /dev/null

    printf 'wait 420\npress SQUARE\nwait 480\nshot 3_after_decline\n' > "$DATA/ui_script.txt"

    run_vita3k_with_cli "$sub" "$work/cli" 20

    local st=0
    if grep -q "Declined incoming device" "$sub/vita.log" 2>/dev/null; then
        echo "OK   [decline] Device declined via Square"
    else
        echo "FAIL [decline] Device decline not logged"
        st=1
    fi
    if grep -qx "$cli_hex" "$sub/ignored_devices.cfg" 2>/dev/null; then
        echo "OK   [decline] ignored_devices.cfg persisted"
    else
        echo "FAIL [decline] ignored_devices.cfg missing declined device"
        st=1
    fi
    if [[ ! -s "$sub/devices.cfg" ]]; then
        echo "OK   [decline] devices.cfg remained empty"
    else
        echo "FAIL [decline] devices.cfg should remain empty"
        st=1
    fi
    rm -rf "$work"
    [[ "$st" -ne 0 ]] && TOTAL_STATUS=1
    return 0
}

case "$MODE" in
    all)
        run_sync
        run_accept
        run_decline
        ;;
    sync) run_sync ;;
    accept) run_accept ;;
    decline) run_decline ;;
esac

echo "Logs and screenshots: $OUT_DIR"
exit "$TOTAL_STATUS"
