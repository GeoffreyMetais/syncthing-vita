#!/usr/bin/env bash
# End-to-end validation of the PS Vita app and syncthing-cli.
#
# Usage: validate.sh [--only stage,...] [--skip stage,...] [--out DIR]
# Stages (in order): quality, host, vita, cli-lan, vita3k, internet
#   quality   cargo fmt --check, cargo clippy --workspace --all-targets, and cargo audit
#   host      cargo test --workspace, zero warnings (host targets)
#   vita      just vpk (zero warnings) + ELF symbol audit (C2-12828-1 prevention)
#   cli-lan   two syncthing-cli daemons find each other by LAN discovery and sync both ways
#   vita3k    the Vita app in Vita3K syncs both ways with syncthing-cli (scripts/vita3k_cli_sync.sh)
#   internet  real Syncthing servers: relay pool, Global Discovery announce + lookup, joining a
#             relay, then a CLI <-> CLI sync through Global Discovery + relay (no LAN, no address)
# Each stage ends PASS, FAIL, BLOCKED (the environment prevents it: no network, no Vita3K...)
# or SKIP. Exit status: 1 if any stage FAILED, else 0. Logs land in DIR (target/e2e-validation).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$ROOT"
OUT="$ROOT/target/e2e-validation"
ONLY=""
SKIP=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --only) ONLY="$2"; shift 2 ;;
        --skip) SKIP="$2"; shift 2 ;;
        --out) OUT="$(realpath -m "$2")"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
rm -rf "$OUT" && mkdir -p "$OUT"
WORK="$(mktemp -d)"
PIDS=()
# Stops the daemons a stage started: a daemon left running would hold UDP 21027 / TCP 22000
# and answer the next stage's peers.
stop_daemons() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
    PIDS=()
}
cleanup() {
    stop_daemons
    rm -rf "$WORK"
}
trap cleanup EXIT

declare -A RESULT
declare -A DETAIL
STAGES=(quality host vita cli-lan vita3k internet)

wanted() {
    [[ -n "$ONLY" && ",$ONLY," != *",$1,"* ]] && return 1
    [[ ",$SKIP," == *",$1,"* ]] && return 1
    return 0
}
result() { RESULT[$1]="$2"; DETAIL[$1]="${3:-}"; echo "==> $1: $2 ${3:-}"; }
CLI="$ROOT/target/debug/syncthing-cli"

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

# Waits up to $2 seconds for the regex $3 in file $1.
wait_log() {
    local file="$1" secs="$2" pattern="$3" i
    for ((i = 0; i < secs * 2; i++)); do
        grep -qE "$pattern" "$file" 2>/dev/null && return 0
        sleep 0.5
    done
    return 1
}

# Two paired CLI homes $WORK/$1 and $WORK/$2 sharing folder "e2e", each with one file.
# Sets A_ID / B_ID. $3: address of A as known by B (`dynamic` or tcp://...).
pair_homes() {
    local a="$WORK/$1" b="$WORK/$2"
    A_ID="$("$CLI" --home "$a" id)"
    B_ID="$("$CLI" --home "$b" id)"
    "$CLI" --home "$a" device add "$B_ID" --name "$2" > /dev/null
    "$CLI" --home "$b" device add "$A_ID" --name "$1" --address "$3" > /dev/null
    "$CLI" --home "$a" folder add e2e "$a/share" --device "$B_ID" > /dev/null
    "$CLI" --home "$b" folder add e2e "$b/share" --device "$A_ID" > /dev/null
    "$CLI" --home "$a" config set rescan-interval 2 > /dev/null
    "$CLI" --home "$b" config set rescan-interval 2 > /dev/null
    echo "from $1 $(date +%s%N)" > "$a/share/from_$1.txt"
    mkdir -p "$b/share/sub"
    echo "from $2 $(date +%s%N)" > "$b/share/sub/from_$2.txt"
}

# Both files present on both sides.
synced_both_ways() {
    cmp -s "$WORK/$1/share/from_$1.txt" "$WORK/$2/share/from_$1.txt" &&
        cmp -s "$WORK/$2/share/sub/from_$2.txt" "$WORK/$1/share/sub/from_$2.txt"
}

stage_quality() {
    local log="$OUT/quality.log"
    if ! cargo fmt --check > "$log" 2>&1; then
        result quality FAIL "(cargo fmt --check failed, see quality.log)"
        return
    fi
    if ! cargo clippy --workspace --all-targets >> "$log" 2>&1; then
        result quality FAIL "(cargo clippy failed, see quality.log)"
        return
    fi
    local clippy_warnings
    clippy_warnings=$(grep -c "^warning:" "$log" || true)
    if command -v cargo-audit > /dev/null 2>&1; then
        if ! (cargo audit >> "$log" 2>&1 || cargo audit --no-fetch >> "$log" 2>&1); then
            result quality FAIL "(cargo audit found vulnerabilities, see quality.log)"
            return
        fi
        result quality PASS "(fmt ok, clippy: $clippy_warnings warning(s), audit: 0 vulnerabilities)"
    else
        result quality PASS "(fmt ok, clippy: $clippy_warnings warning(s), cargo-audit not installed)"
    fi
}

stage_host() {
    local log="$OUT/host.log"
    if ! cargo test --workspace > "$log" 2>&1; then
        result host FAIL "(cargo test failed, see host.log)"
        return
    fi
    local warnings passed
    warnings=$(grep -c "^warning" "$log")
    passed=$(grep -E "^test result" "$log" | awk '{s += $4} END {print s}')
    if [[ "$warnings" -gt 0 ]]; then
        result host FAIL "($warnings compiler warning(s), see host.log)"
    else
        result host PASS "($passed tests, 0 warnings)"
    fi
}

stage_vita() {
    local log="$OUT/vita.log"
    if ! command -v just > /dev/null || [[ ! -d "${VITASDK:-/usr/local/vitasdk}" ]]; then
        result vita BLOCKED "(VitaSDK or just missing: see the vita-dev-environment skill)"
        return
    fi
    if ! just vpk > "$log" 2>&1; then
        result vita FAIL "(just vpk failed, see vita.log)"
        return
    fi
    if grep -q "^warning" "$log"; then
        result vita FAIL "(compiler warnings in the Vita build, see vita.log)"
        return
    fi
    local nm="${VITASDK:-/usr/local/vitasdk}/bin/arm-vita-eabi-nm"
    local symbols
    symbols="$("$nm" -n target/armv7-sony-vita-newlibeabihf/release/vita-syncthing-app.elf)"
    if grep -q "R _newlib_heap_size_user" <<< "$symbols" &&
        grep -q "R sceUserMainThreadStackSize" <<< "$symbols"; then
        result vita PASS "(SYNCTHING.vpk built, heap / stack symbols exported)"
    else
        result vita FAIL "(_newlib_heap_size_user or sceUserMainThreadStackSize missing)"
    fi
}

stage_cli_lan() {
    pair_homes lan-a lan-b dynamic
    local pa pb
    pa=$(free_port)
    pb=$(free_port)
    "$CLI" --home "$WORK/lan-a" run --listen "0.0.0.0:$pa" --no-global-discovery --no-relays \
        --exit-after 90 > "$OUT/cli-lan-a.log" 2>&1 &
    PIDS+=($!)
    sleep 1
    timeout 100 "$CLI" --home "$WORK/lan-b" run --listen "0.0.0.0:$pb" --no-global-discovery \
        --no-relays --exit-when-synced > "$OUT/cli-lan-b.log" 2>&1
    sleep 2
    if synced_both_ways lan-a lan-b &&
        grep -qE "LAN: lan-[ab] announces" "$OUT/cli-lan-a.log" "$OUT/cli-lan-b.log"; then
        result cli-lan PASS "(found by LAN discovery, synced both ways)"
    elif ! grep -q "LAN:" "$OUT/cli-lan-a.log" "$OUT/cli-lan-b.log"; then
        result cli-lan BLOCKED "(no LAN announcement received: UDP broadcast unavailable here?)"
    else
        result cli-lan FAIL "(see cli-lan-a.log / cli-lan-b.log)"
    fi
}

stage_vita3k() {
    if ! command -v Vita3K > /dev/null && [[ ! -x "$HOME/.local/bin/Vita3K" ]]; then
        result vita3k BLOCKED "(Vita3K not installed: see the vita-dev-environment skill)"
        return
    fi
    if ! CARGO_FEATURES=ui-script just vpk > "$OUT/vita3k-build.log" 2>&1; then
        result vita3k FAIL "(ui-script VPK build failed, see vita3k-build.log)"
        return
    fi
    if ! ./scripts/vita3k_cli_sync.sh 75 "$OUT/vita3k" > "$OUT/vita3k.log" 2>&1; then
        result vita3k FAIL "($(grep -c '^FAIL' "$OUT/vita3k.log") outbound check(s) failed, see vita3k.log)"
    elif ! ./scripts/vita3k_inbound_e2e.sh all "$OUT/vita3k-inbound" > "$OUT/vita3k-inbound.log" 2>&1; then
        result vita3k FAIL "($(grep -c '^FAIL' "$OUT/vita3k-inbound.log") inbound check(s) failed, see vita3k-inbound.log)"
    else
        result vita3k PASS "(Vita <-> CLI outbound + inbound + DeviceOffer E2E passed; screenshots in vita3k/ and vita3k-inbound/)"
    fi
    echo "    note: target/.../SYNCTHING.vpk now contains the ui-script feature: rebuild with \`just vpk\` before any release"
}

stage_internet() {
    local home="$WORK/net-a" log="$OUT/internet-a.log" port
    if ! "$CLI" --home "$home" relays > "$OUT/internet-relays.txt" 2> "$OUT/internet-relays.err"; then
        result internet BLOCKED "(relay pool unreachable: $(tail -1 "$OUT/internet-relays.err"))"
        return
    fi
    local relays notes=()
    relays=$(grep -c "^relay://" "$OUT/internet-relays.txt")
    notes+=("pool: $relays relays")

    # Device A: announces itself and joins a relay; B knows it only by ID.
    pair_homes net-a net-b dynamic
    port=$(free_port)
    "$CLI" --home "$home" run --listen "0.0.0.0:$port" --no-local-discovery --exit-after 240 \
        > "$log" 2>&1 &
    PIDS+=($!)
    if ! wait_log "$log" 30 "Global Discovery: (announced|cannot announce)"; then
        result internet FAIL "(${notes[*]}; no announcement attempt within 30 s)"
        return
    fi
    if ! grep -q "Global Discovery: announced" "$log"; then
        result internet FAIL "(${notes[*]}; announce refused: $(grep -m1 'cannot announce' "$log"))"
        return
    fi
    if ! "$CLI" --home "$home" lookup "$A_ID" > "$OUT/internet-lookup.txt" 2>&1 ||
        ! grep -q "tcp://" "$OUT/internet-lookup.txt"; then
        result internet FAIL "(${notes[*]}; announced but the lookup does not find it)"
        return
    fi
    notes+=("announce + lookup ok: $(grep -m1 'tcp://' "$OUT/internet-lookup.txt" | xargs)")

    if ! wait_log "$log" 120 "Reachable through relay"; then
        local why
        why=$(grep -m1 "Cannot join relay" "$log" || echo "no relay joined")
        result internet BLOCKED "(${notes[*]}; relays unreachable from here: $why)"
        return
    fi
    notes+=("$(grep -m1 'Reachable through relay' "$log" | sed 's/^\[[^]]*\] //')")
    # Wait for the relay URI to be announced, then sync B -> A over the Internet path only.
    wait_log "$log" 30 "announced .*relay://" || true
    timeout 150 "$CLI" --home "$WORK/net-b" run --listen none --no-local-discovery \
        --exit-when-synced > "$OUT/internet-b.log" 2>&1
    sleep 2
    if synced_both_ways net-a net-b; then
        result internet PASS "(${notes[*]}; synced both ways through Global Discovery + relay)"
    else
        result internet FAIL "(${notes[*]}; Internet sync failed, see internet-b.log)"
    fi
}

cargo build -q -p syncthing-cli || { echo "syncthing-cli does not build" >&2; exit 1; }
for stage in "${STAGES[@]}"; do
    if wanted "$stage"; then
        echo "==> $stage..."
        "stage_${stage//-/_}"
        stop_daemons
    else
        result "$stage" SKIP
    fi
done

echo
echo "Summary (logs in $OUT):"
status=0
for stage in "${STAGES[@]}"; do
    printf '  %-9s %-8s %s\n' "$stage" "${RESULT[$stage]}" "${DETAIL[$stage]}"
    [[ "${RESULT[$stage]}" == FAIL ]] && status=1
done
exit $status
