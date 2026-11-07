#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <hex_offset_or_psp2core_filename>" >&2
    exit 1
fi

INPUT="$1"
if [[ "${INPUT}" =~ 0x([0-9a-fA-F]+) ]]; then
    OFFSET_HEX="${BASH_REMATCH[1]}"
else
    OFFSET_HEX="${INPUT#0x}"
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
ELF_PATH="${ROOT_DIR}/target/armv7-sony-vita-newlibeabihf/release/vita-syncthing-app.elf"
VITASDK="${VITASDK:-/usr/local/vitasdk}"

if [[ ! -f "${ELF_PATH}" ]]; then
    echo "ERROR: ELF file not found at ${ELF_PATH}. Run 'just vpk' first." >&2
    exit 1
fi

TEXT_BASE=$((0x81000000))
OFFSET_VAL=$((16#${OFFSET_HEX}))
# Clear Thumb bit (bit 0)
INSTR_ADDR=$(( (TEXT_BASE + OFFSET_VAL) & ~1 ))
START_ADDR=$(( INSTR_ADDR - 64 ))
STOP_ADDR=$(( INSTR_ADDR + 64 ))

printf "Crash Offset : 0x%08x\n" "${OFFSET_VAL}"
printf "ELF Address  : 0x%08x (Thumb-aligned)\n\n" "${INSTR_ADDR}"

echo "=== Nearest Symbols ==="
"${VITASDK}/bin/arm-vita-eabi-nm" -n "${ELF_PATH}" | grep -E "^[0-9a-fA-F]{8}" | awk -v target="$(printf "%08x" "${INSTR_ADDR}")" '
    $1 <= target { prev=$0 }
    $1 > target { if (prev) print prev; print $0; exit }
'

echo ""
echo "=== Disassembly Window (0x$(printf "%08x" "${START_ADDR}") .. 0x$(printf "%08x" "${STOP_ADDR}")) ==="
"${VITASDK}/bin/arm-vita-eabi-objdump" -d \
    --start-address="$(printf "0x%08x" "${START_ADDR}")" \
    --stop-address="$(printf "0x%08x" "${STOP_ADDR}")" \
    "${ELF_PATH}"
