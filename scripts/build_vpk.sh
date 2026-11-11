#!/usr/bin/env bash
set -euo pipefail

APP_NAME="vita-syncthing-app"
TITLE_ID="SYNCTHING"
TITLE_NAME="Syncthing"
TARGET="armv7-sony-vita-newlibeabihf"
PROFILE="${PROFILE:-release}"

if [[ -z "${VITASDK:-}" ]]; then
    echo "Error: VITASDK environment variable is not set." >&2
    exit 1
fi

BUILD_DIR="target/${TARGET}/${PROFILE}"
PKG_DIR="${BUILD_DIR}/pkg"
SCE_SYS_DIR="${PKG_DIR}/sce_sys"

echo "[1/5] Compiling Rust (target: ${TARGET}, profile: ${PROFILE})..."
cargo +nightly build \
    -Z build-std=std,panic_abort \
    --package "${APP_NAME}" \
    --target "${TARGET}" ${CARGO_FEATURES:+--features ${CARGO_FEATURES}} \
    $( [[ "${PROFILE}" == "release" ]] && echo "--release" )

mkdir -p "${SCE_SYS_DIR}"

echo "[2/5] Converting ELF -> Vita ELF (vita-elf-create)..."
ELF_FILE="${BUILD_DIR}/${APP_NAME}.elf"
[[ -f "${ELF_FILE}" ]] || ELF_FILE="${BUILD_DIR}/${APP_NAME}"
vita-elf-create "${ELF_FILE}" "${BUILD_DIR}/${APP_NAME}.velf"

echo "[3/5] Generating eboot.bin (vita-make-fself)..."
# -c: compressed full-permission homebrew eboot (required for full ux0: access & sysmodules)
vita-make-fself -c "${BUILD_DIR}/${APP_NAME}.velf" "${PKG_DIR}/eboot.bin"

echo "[4/5] Generating param.sfo (vita-mksfoex)..."
# ATTRIBUTE2=12 enables extended memory mode (+109 MB on Vita) if needed
vita-mksfoex -s TITLE_ID="${TITLE_ID}" -d ATTRIBUTE2=12 "${TITLE_NAME}" "${SCE_SYS_DIR}/param.sfo"

echo "[5/5] Packaging VPK archive (vita-pack-vpk)..."
VPK_PATH="${BUILD_DIR}/${TITLE_ID}.vpk"
vita-pack-vpk -s "${SCE_SYS_DIR}/param.sfo" -b "${PKG_DIR}/eboot.bin" "${VPK_PATH}"

echo "✅ Packaging complete: ${VPK_PATH}"
