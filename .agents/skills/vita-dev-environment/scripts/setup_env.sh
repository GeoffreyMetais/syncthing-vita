#!/usr/bin/env bash
set -euo pipefail

# setup_env.sh: Automates VitaSDK & Vita3K emulator environment verification and installation
# Usage:
#   ./setup_env.sh --check-all
#   ./setup_env.sh --install-vitasdk
#   ./setup_env.sh --install-vita3k
#   ./setup_env.sh --install-firmware

VITASDK_DIR="${VITASDK:-/usr/local/vitasdk}"
VITA3K_BIN="${HOME}/.local/bin/Vita3K"

check_tools() {
    echo "=== Checking PS Vita Development Environment ==="
    
    # 1. Check VitaSDK
    if [[ -x "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" ]]; then
        echo "✅ VitaSDK installed at: ${VITASDK_DIR}"
        "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" --version | head -n 1
    else
        echo "❌ VitaSDK not found at: ${VITASDK_DIR}"
    fi

    # 2. Check Packaging Tools
    for tool in vita-elf-create vita-make-fself vita-mksfoex vita-pack-vpk; do
        if command -v "$tool" >/dev/null 2>&1 || [[ -x "${VITASDK_DIR}/bin/$tool" ]]; then
            echo "✅ $tool: present"
        else
            echo "❌ $tool: missing"
        fi
    done

    # 3. Check Rust Nightly & Target
    if rustup toolchain list | grep -q "nightly"; then
        echo "✅ Rust nightly toolchain installed"
    else
        echo "❌ Rust nightly missing (run: rustup toolchain install nightly)"
    fi

    if rustup component list --toolchain nightly | grep -q "rust-src (installed)"; then
        echo "✅ rust-src component installed"
    else
        echo "❌ rust-src component missing (run: rustup component add rust-src --toolchain nightly)"
    fi

    # 4. Check Vita3K Emulator
    if [[ -x "${VITA3K_BIN}" ]]; then
        echo "✅ Vita3K emulator installed at: ${VITA3K_BIN}"
    else
        echo "❌ Vita3K emulator not found at: ${VITA3K_BIN}"
    fi
}

install_vitasdk() {
    echo "=== Installing VitaSDK to ${VITASDK_DIR} ==="
    if [[ -x "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" ]]; then
        echo "✅ VitaSDK already installed at: ${VITASDK_DIR}"
        return
    fi
    # The official bootstrap installs the newest supported series with pacman (vdpm),
    # verifying the signed channel manifest. It refuses an existing destination.
    if [[ -d "${VITASDK_DIR}" ]] && [[ -z "$(ls -A "${VITASDK_DIR}")" ]]; then
        rmdir "${VITASDK_DIR}"
    fi
    local src
    src="$(mktemp -d)"
    git clone -q --depth 1 https://github.com/vitasdk/vdpm "${src}/vdpm"
    (cd "${src}/vdpm" && VITASDK="${VITASDK_DIR}" ./bootstrap-vitasdk.sh)
    rm -rf "${src}"
    echo "✅ VitaSDK installed. Ensure VITASDK=${VITASDK_DIR} and PATH=\"\$VITASDK/bin:\$PATH\" are exported."
}

install_vita3k() {
    echo "=== Installing Vita3K to ${VITA3K_BIN} ==="
    local bin_dir
    bin_dir="$(dirname "${VITA3K_BIN}")"
    mkdir -p "${bin_dir}"
    curl -fL https://github.com/Vita3K/Vita3K/releases/download/continuous/Vita3K-x86_64.AppImage \
        -o "${bin_dir}/Vita3K.AppImage"
    chmod +x "${bin_dir}/Vita3K.AppImage"
    # Extract the AppImage so it also runs in containers without FUSE.
    rm -rf "${bin_dir}/vita3k-appdir" "${bin_dir}/squashfs-root"
    (cd "${bin_dir}" && ./Vita3K.AppImage --appimage-extract >/dev/null && mv squashfs-root vita3k-appdir)
    printf '#!/bin/sh\nexec "%s/vita3k-appdir/AppRun" "$@"\n' "${bin_dir}" > "${VITA3K_BIN}"
    chmod +x "${VITA3K_BIN}"
    echo "✅ Vita3K installed at: ${VITA3K_BIN}"
    echo "   Headless: xvfb-run -a env XDG_RUNTIME_DIR=/tmp/xdg LIBGL_ALWAYS_SOFTWARE=1 MESA_GL_VERSION_OVERRIDE=4.6 \\"
    echo "             ${VITA3K_BIN} -z --frontend sdl -B OpenGL -l 2 -r SYNCTHING"
}

install_firmware() {
    if [[ ! -x "${VITA3K_BIN}" ]]; then
        echo "❌ Vita3K binary not found. Install Vita3K first." >&2
        exit 1
    fi
    echo "=== Downloading & Installing Sony PS Vita Firmware into Vita3K ==="
    # Sony's update servers only serve these files over plain HTTP: this step fails
    # behind HTTPS-only proxies. Homebrew such as VitaSyncthing runs without firmware.
    mkdir -p /tmp/vita-fw
    curl -fL http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP -o /tmp/vita-fw/PSP2UPDAT.PUP
    curl -fL http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP
    
    "${VITA3K_BIN}" -z --firmware /tmp/vita-fw/PSP2UPDAT.PUP
    "${VITA3K_BIN}" -z --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP
    rm -rf /tmp/vita-fw
    echo "✅ Firmware installation complete."
}

ACTION="${1:---check-all}"
case "$ACTION" in
    --check-all)
        check_tools
        ;;
    --install-vitasdk)
        install_vitasdk
        ;;
    --install-vita3k)
        install_vita3k
        ;;
    --install-firmware)
        install_firmware
        ;;
    *)
        echo "Usage: $0 [--check-all | --install-vitasdk | --install-vita3k | --install-firmware]"
        exit 1
        ;;
esac
