#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "${ROOT_DIR}"

if [[ -z "${VITASDK:-}" ]]; then
    if [[ -d "${HOME}/vitasdk" ]]; then
        export VITASDK="${HOME}/vitasdk"
    elif [[ -d "/usr/local/vitasdk" ]]; then
        export VITASDK="/usr/local/vitasdk"
    elif command -v arm-vita-eabi-gcc >/dev/null 2>&1; then
        export VITASDK="$(dirname "$(dirname "$(command -v arm-vita-eabi-gcc)")")"
    else
        export VITASDK="/usr/local/vitasdk"
    fi
fi
export PATH="${VITASDK}/bin:${HOME}/.local/bin:${PATH}"
ELF_PATH="target/armv7-sony-vita-newlibeabihf/release/vita-syncthing-app.elf"
VPK_PATH="target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk"
VITA3K_APP_DIR="${HOME}/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING"

echo "==> [1/4] Running host unit tests..."
cargo test --workspace

echo "==> [2/4] Building PS Vita release VPK (SYNCTHING.vpk)..."
if command -v just >/dev/null 2>&1; then
    just vpk
else
    ./scripts/build_vpk.sh
fi

echo "==> [3/4] Auditing hardware-critical ELF symbols (C2-12828-1 prevention)..."
SYMBOLS="$("${VITASDK}/bin/arm-vita-eabi-nm" -n "${ELF_PATH}")"

if ! grep -q "R _newlib_heap_size_user" <<< "${SYMBOLS}"; then
    echo "ERROR: _newlib_heap_size_user is missing or weak in ${ELF_PATH}!" >&2
    exit 1
fi
if ! grep -q "R sceUserMainThreadStackSize" <<< "${SYMBOLS}"; then
    echo "ERROR: sceUserMainThreadStackSize is missing in ${ELF_PATH}!" >&2
    exit 1
fi
echo "    Found exported symbols:"
grep -E "_newlib_heap_size_user|sceUserMainThread" <<< "${SYMBOLS}"

if [[ -x "${VITA3K_BIN:-${HOME}/.local/bin/Vita3K}" ]]; then
    echo "==> [4/4] Installing SYNCTHING.vpk into Vita3K (ux0:/app/SYNCTHING)..."
    mkdir -p "${VITA3K_APP_DIR}"
    unzip -qo "${VPK_PATH}" -d "${VITA3K_APP_DIR}"
    echo "    Installed to ${VITA3K_APP_DIR}"
fi

if [[ "${1:-}" == "--ftp" && -n "${2:-}" ]]; then
    FTP_TARGET="${2}"
    echo "==> Uploading ${VPK_PATH} to ftp://${FTP_TARGET}/ux0:/downloads/SYNC00001.vpk ..."
    curl -T "${VPK_PATH}" "ftp://${FTP_TARGET}/ux0:/downloads/SYNC00001.vpk"
    echo "    Upload complete. Install ux0:/downloads/SYNC00001.vpk from VitaShell."
fi

echo "✅ Verification complete: ${VPK_PATH}"
