#!/usr/bin/env bash
# Static audit of the background sync plugin modules (AGENTS.md §8). Nothing here can run them:
# Vita3K does not load taiHEN plugins into SceShell. It checks what a crash in SceShell would
# otherwise reveal on real hardware.
#
# Usage: scripts/audit_plugin.sh <build dir> [<vpk>]
#   <build dir>: target/armv7-sony-vita-newlibeabihf/<profile> (holds plugin-build/ and plugins/)
#   <vpk>:       also check that the package bundles both modules under plugins/
set -euo pipefail

BUILD_DIR="${1:?usage: audit_plugin.sh <build dir> [<vpk>]}"
VPK="${2:-}"
OBJ_DIR="${BUILD_DIR}/plugin-build"
OUT_DIR="${BUILD_DIR}/plugins"
if [[ -n "${VITASDK:-}" ]]; then
    export PATH="${VITASDK}/bin:${PATH}"
fi
NM="arm-vita-eabi-nm"
OBJDUMP="arm-vita-eabi-objdump"

# Must match WORKER_HEAP_BYTES in crates/syncthing-vita-worker/src/vita/mod.rs.
EXPECTED_HEAP_BYTES=$((8 * 1024 * 1024))
# The launcher stays resident in SceShell: keep it tiny.
LAUNCHER_MAX_SUPRX_BYTES=$((32 * 1024))

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

check_module() {
    local name="$1"
    local elf="${OBJ_DIR}/${name}.elf"
    local suprx="${OUT_DIR}/${name}.suprx"
    [[ -f "${elf}" ]] || fail "${elf} is missing"
    [[ -f "${suprx}" ]] || fail "${suprx} is missing"
    [[ "$(head -c 4 "${suprx}" | od -An -c | tr -d ' ')" == 'SCE\0' ]] ||
        fail "${suprx} is not a SELF"
    local symbols
    symbols="$("${NM}" "${elf}")"
    grep -q " T module_start$" <<< "${symbols}" || fail "${name}: module_start is not exported"
    grep -q " T module_stop$" <<< "${symbols}" || fail "${name}: module_stop is not exported"
    if grep -q " T main$" <<< "${symbols}"; then
        fail "${name}: a module must not have a main"
    fi
    echo "    ${name}: module_start / module_stop exported ($(stat -c %s "${suprx}") bytes)"
}

check_module syncthing_launcher
check_module syncthing_worker

# Launcher: no C library, no heap (it never allocates in SceShell).
LAUNCHER_SYMBOLS="$("${NM}" "${OBJ_DIR}/syncthing_launcher.elf")"
if grep -qE " T (malloc|_init_vita_heap|_sbrk_r)$" <<< "${LAUNCHER_SYMBOLS}"; then
    fail "syncthing_launcher links a heap"
fi
[[ "$(stat -c %s "${OUT_DIR}/syncthing_launcher.suprx")" -le "${LAUNCHER_MAX_SUPRX_BYTES}" ]] ||
    fail "syncthing_launcher.suprx is larger than ${LAUNCHER_MAX_SUPRX_BYTES} bytes"

# Worker: explicit newlib heap (otherwise newlib asks SceShell for 128 MiB and the first
# allocation aborts), and the shim's overrides of the paths that would end SceShell.
WORKER_ELF="${OBJ_DIR}/syncthing_worker.elf"
WORKER_SYMBOLS="$("${NM}" -n "${WORKER_ELF}")"
HEAP_LINE="$(grep " R _newlib_heap_size_user$" <<< "${WORKER_SYMBOLS}")" ||
    fail "syncthing_worker: _newlib_heap_size_user is missing or not read-only"
HEAP_ADDR="0x${HEAP_LINE%% *}"
HEAP_WORD="$("${OBJDUMP}" -s -j .rodata --start-address="${HEAP_ADDR}" \
    --stop-address="$((HEAP_ADDR + 4))" "${WORKER_ELF}" | awk '/^ [0-9a-f]+ /{print $2; exit}')"
HEAP_BYTES=$((16#${HEAP_WORD:6:2}${HEAP_WORD:4:2}${HEAP_WORD:2:2}${HEAP_WORD:0:2}))
[[ "${HEAP_BYTES}" -eq "${EXPECTED_HEAP_BYTES}" ]] ||
    fail "syncthing_worker: heap is ${HEAP_BYTES} bytes, expected ${EXPECTED_HEAP_BYTES}"
echo "    syncthing_worker: _newlib_heap_size_user = ${HEAP_BYTES} bytes"
grep -q " T _free_vita_newlib$" <<< "${WORKER_SYMBOLS}" ||
    fail "syncthing_worker: the shim's _free_vita_newlib is missing (_exit would end SceShell)"
WORKER_MAP="${OBJ_DIR}/syncthing_worker.map"
if grep -q "lib_a-abort.o" "${WORKER_MAP}"; then
    fail "syncthing_worker: newlib's abort is linked (a panic would end SceShell)"
fi
if grep -q "crt0.o" "${WORKER_MAP}"; then
    fail "syncthing_worker: crt0 is linked into a module"
fi
echo "    syncthing_worker: abort / _exit end the failing thread, never SceShell"
# Libraries that may be missing from SceShell when the worker loads must be imported weakly:
# a strong import of an absent library makes sceKernelLoadStartModule fail.
for lib in SceNotificationUtil SceRegistryMgr; do
    if grep -q "lib${lib}_stub\.a(" "${WORKER_MAP}"; then
        fail "syncthing_worker: ${lib} is imported strongly (use lib${lib}_stub_weak)"
    fi
    grep -q "lib${lib}_stub_weak\.a(" "${WORKER_MAP}" ||
        fail "syncthing_worker: the weak ${lib} import is missing (offer notification)"
done
echo "    syncthing_worker: SceNotificationUtil / SceRegistryMgr imported weakly"

if [[ -n "${VPK}" ]]; then
    LISTING="$(unzip -l "${VPK}")"
    for name in syncthing_launcher syncthing_worker; do
        grep -q " plugins/${name}.suprx$" <<< "${LISTING}" ||
            fail "${VPK} does not bundle plugins/${name}.suprx"
        cmp -s <(unzip -p "${VPK}" "plugins/${name}.suprx") "${OUT_DIR}/${name}.suprx" ||
            fail "${VPK}: plugins/${name}.suprx differs from ${OUT_DIR}/${name}.suprx"
    done
    echo "    ${VPK}: plugins/syncthing_launcher.suprx and plugins/syncthing_worker.suprx bundled"
fi
echo "    Plugin audit passed."
