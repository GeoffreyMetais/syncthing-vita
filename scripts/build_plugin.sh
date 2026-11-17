#!/usr/bin/env bash
# Builds the two taiHEN user modules of the background sync plugin (AGENTS.md §8):
#   - syncthing_launcher.suprx (stage 1, C, plugin/launcher): resident in SceShell, no libc;
#   - syncthing_worker.suprx (stage 2, Rust std + newlib, crates/syncthing-vita-worker): one
#     sync cycle, loaded and unloaded by the launcher.
# Output: target/armv7-sony-vita-newlibeabihf/<profile>/plugins/, audited by
# scripts/audit_plugin.sh. scripts/build_vpk.sh bundles them as app0:plugins/<name>.
#
# Both are built like any homebrew SELF with `vita-make-fself -c` (compressed, *without* `-s`:
# `-s` would make them "safe" SELFs, which cannot use SceAppMgr's ForShell calls nor load
# another module).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

TARGET="armv7-sony-vita-newlibeabihf"
PROFILE="${PROFILE:-release}"

if [[ -z "${VITASDK:-}" ]]; then
    if [[ -d "${HOME}/vitasdk" ]]; then
        export VITASDK="${HOME}/vitasdk"
    elif [[ -d "/usr/local/vitasdk" ]]; then
        export VITASDK="/usr/local/vitasdk"
    elif command -v arm-vita-eabi-gcc >/dev/null 2>&1; then
        export VITASDK="$(dirname "$(dirname "$(command -v arm-vita-eabi-gcc)")")"
    else
        echo "Error: VITASDK environment variable is not set." >&2
        exit 1
    fi
fi
export PATH="${VITASDK}/bin:${PATH}"

BUILD_DIR="target/${TARGET}/${PROFILE}"
OBJ_DIR="${BUILD_DIR}/plugin-build"
OUT_DIR="${BUILD_DIR}/plugins"
mkdir -p "${OBJ_DIR}" "${OUT_DIR}"

CPU_FLAGS=(-mcpu=cortex-a9 -mfpu=neon-fp16 -mfloat-abi=hard)
# A module has no crt0: no `main`, no `_start` from the C library.
MODULE_LDFLAGS=(-Wl,-q -Wl,-z,noexecstack -Wl,--gc-sections -Wl,-e,module_start
    -Wl,--undefined=module_stop)

echo "[1/3] syncthing_launcher.suprx (C, no libc)..."
LAUNCHER_ELF="${OBJ_DIR}/syncthing_launcher.elf"
arm-vita-eabi-gcc -Os -std=gnu11 -Wall -Wextra -Werror -ffreestanding -fno-builtin -nostdlib \
    -mthumb "${CPU_FLAGS[@]}" -ffunction-sections -fdata-sections \
    -Iplugin/include plugin/launcher/src/*.c \
    "${MODULE_LDFLAGS[@]}" -Wl,-Map,"${OBJ_DIR}/syncthing_launcher.map" -o "${LAUNCHER_ELF}" \
    -lSceLibKernel_stub -lSceKernelThreadMgr_stub -lSceKernelModulemgr_stub \
    -lSceIofilemgr_stub -lSceAppMgr_stub -lSceNetCtl_stub -lSceProcessmgr_stub \
    -lSceSysmodule_stub -lgcc
vita-elf-create -e plugin/launcher/exports.yml "${LAUNCHER_ELF}" "${OBJ_DIR}/syncthing_launcher.velf" >/dev/null
vita-make-fself -c "${OBJ_DIR}/syncthing_launcher.velf" "${OUT_DIR}/syncthing_launcher.suprx" >/dev/null

echo "[2/3] syncthing_worker.suprx (Rust + syncthing-core, newlib)..."
cargo +nightly build \
    -Z build-std=std,panic_abort \
    --package syncthing-vita-worker \
    --target "${TARGET}" \
    $( [[ "${PROFILE}" == "release" ]] && echo "--release" )
CRT_DIR="$(dirname "$(arm-vita-eabi-gcc -print-file-name=crti.o)")"
WORKER_ELF="${OBJ_DIR}/syncthing_worker.elf"
# crti / crtbegin / crtend / crtn provide `_init`, `_fini` and the frame registration that
# newlib's `__libc_init_array` / `__libc_fini_array` expect; crt0 (with its `main`) is left out:
# the shim (crates/syncthing-vita-worker/src/vita) initializes newlib in `module_start`.
arm-vita-eabi-gcc -nostartfiles "${CPU_FLAGS[@]}" "${MODULE_LDFLAGS[@]}" \
    -Wl,--undefined=_newlib_heap_size_user -Wl,-Map,"${OBJ_DIR}/syncthing_worker.map" \
    -o "${WORKER_ELF}" \
    "${CRT_DIR}/crti.o" "${CRT_DIR}/crtbegin.o" \
    "${BUILD_DIR}/libsyncthing_vita_worker.a" \
    -lpthread -lm -lc \
    -lSceSysmodule_stub -lSceSysmem_stub -lSceNet_stub -lSceNetCtl_stub \
    -lSceKernelThreadMgr_stub -lSceKernelModulemgr_stub -lSceLibKernel_stub \
    -lSceIofilemgr_stub -lSceProcessmgr_stub -lSceRtc_stub \
    -lSceNotificationUtil_stub_weak -lSceRegistryMgr_stub_weak -lgcc \
    "${CRT_DIR}/crtend.o" "${CRT_DIR}/crtn.o"
vita-elf-create -e plugin/worker/exports.yml "${WORKER_ELF}" "${OBJ_DIR}/syncthing_worker.velf" >/dev/null
vita-make-fself -c "${OBJ_DIR}/syncthing_worker.velf" "${OUT_DIR}/syncthing_worker.suprx" >/dev/null

echo "[3/3] Auditing the modules..."
./scripts/audit_plugin.sh "${BUILD_DIR}"

echo "✅ Plugin modules: ${OUT_DIR}/"
ls -l "${OUT_DIR}"
