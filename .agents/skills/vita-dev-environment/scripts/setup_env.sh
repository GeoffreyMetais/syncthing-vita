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
    sudo mkdir -p "${VITASDK_DIR}"
    sudo chown -R "$(whoami):$(whoami)" "${VITASDK_DIR}"
    curl -sL https://github.com/vitasdk/autobuilds/releases/download/master-linux/vitasdk-x86_64-linux-ubuntu-latest.tar.bz2 | tar -xj -C "${VITASDK_DIR}" --strip-components=1
    echo "✅ VitaSDK extracted. Ensure VITASDK=${VITASDK_DIR} and PATH=\"\$VITASDK/bin:\$PATH\" are exported."
}

install_vita3k() {
    echo "=== Installing Vita3K to ${VITA3K_BIN} ==="
    mkdir -p "$(dirname "${VITA3K_BIN}")"
    curl -fL https://github.com/Vita3K/Vita3K/releases/download/continuous/Vita3K-x86_64.AppImage -o "${VITA3K_BIN}"
    chmod +x "${VITA3K_BIN}"
    echo "✅ Vita3K installed at: ${VITA3K_BIN}"
}

install_firmware() {
    if [[ ! -x "${VITA3K_BIN}" ]]; then
        echo "❌ Vita3K binary not found. Install Vita3K first." >&2
        exit 1
    fi
    echo "=== Downloading & Installing Sony PS Vita Firmware into Vita3K ==="
    mkdir -p /tmp/vita-fw
    curl -fL http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP -o /tmp/vita-fw/PSP2UPDAT.PUP
    curl -fL http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP
    
    "${VITA3K_BIN}" --console 1 --firmware /tmp/vita-fw/PSP2UPDAT.PUP
    "${VITA3K_BIN}" --console 1 --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP
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
