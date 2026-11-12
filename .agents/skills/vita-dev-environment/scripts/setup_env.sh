#!/usr/bin/env bash
set -euo pipefail

# setup_env.sh: Automates VitaSDK, Rust cross-compilation, and Vita3K emulator setup.
#
# Usage:
#   ./scripts/setup_env.sh --check-all
#   ./scripts/setup_env.sh --install-all
#   ./scripts/setup_env.sh --install-vitasdk
#   ./scripts/setup_env.sh --install-vita3k
#   ./scripts/setup_env.sh --install-firmware
#   ./scripts/setup_env.sh --install-rust
#   ./scripts/setup_env.sh --install-just

detect_vitasdk_dir() {
    if [[ -n "${VITASDK:-}" ]]; then
        echo "${VITASDK}"
    elif [[ -d "${HOME}/vitasdk" ]]; then
        echo "${HOME}/vitasdk"
    elif [[ -d "/usr/local/vitasdk" ]]; then
        echo "/usr/local/vitasdk"
    elif [[ -w "/usr/local" ]]; then
        echo "/usr/local/vitasdk"
    else
        echo "${HOME}/vitasdk"
    fi
}

VITASDK_DIR="$(detect_vitasdk_dir)"
VITA3K_BIN="${HOME}/.local/bin/Vita3K"
VITA3K_APPDIR="${HOME}/.local/share/vita3k-appdir"

# Ensure user-writable Rust and local bin paths are configured
setup_user_env() {
    mkdir -p "${HOME}/.local/bin"
    if [[ -d "${RUSTUP_HOME:-/usr/local/rustup}" && ! -w "${RUSTUP_HOME:-/usr/local/rustup}" ]]; then
        export RUSTUP_HOME="${HOME}/.rustup"
    fi
    if [[ -d "${CARGO_HOME:-/usr/local/cargo}" && ! -w "${CARGO_HOME:-/usr/local/cargo}" ]]; then
        export CARGO_HOME="${HOME}/.cargo"
    fi
    mkdir -p "${RUSTUP_HOME:-${HOME}/.rustup}" "${CARGO_HOME:-${HOME}/.cargo}"
    export PATH="${HOME}/.local/bin:${CARGO_HOME:-${HOME}/.cargo}/bin:${VITASDK_DIR}/bin:${PATH}"
}

check_tools() {
    setup_user_env
    echo "=== Checking PS Vita Development Environment ==="
    
    # 1. Check VitaSDK
    if [[ -x "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" ]]; then
        echo "✅ VitaSDK installed at: ${VITASDK_DIR}"
        "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" --version | head -n 1
    elif command -v arm-vita-eabi-gcc >/dev/null 2>&1; then
        V_PATH="$(command -v arm-vita-eabi-gcc)"
        echo "✅ VitaSDK found in PATH: ${V_PATH}"
        "${V_PATH}" --version | head -n 1
    else
        echo "❌ VitaSDK not found at: ${VITASDK_DIR} (run: $0 --install-vitasdk)"
    fi

    # 2. Check Packaging Tools
    local missing_tools=0
    for tool in vita-elf-create vita-make-fself vita-mksfoex vita-pack-vpk; do
        if command -v "$tool" >/dev/null 2>&1 || [[ -x "${VITASDK_DIR}/bin/$tool" ]]; then
            echo "✅ $tool: present"
        else
            echo "❌ $tool: missing"
            missing_tools=$((missing_tools + 1))
        fi
    done

    # 3. Check Just task runner
    if command -v just >/dev/null 2>&1; then
        echo "✅ just: present ($(just --version))"
    else
        echo "❌ just: missing (run: $0 --install-just)"
    fi

    # 4. Check Rust Nightly & Target
    if rustup toolchain list 2>/dev/null | grep -q "nightly"; then
        echo "✅ Rust nightly toolchain installed"
    else
        echo "❌ Rust nightly missing (run: $0 --install-rust)"
    fi

    if rustup component list --toolchain nightly 2>/dev/null | grep -q "rust-src (installed)"; then
        echo "✅ rust-src component installed"
    else
        echo "❌ rust-src component missing (run: $0 --install-rust)"
    fi

    # 5. Check Vita3K Emulator
    if [[ -x "${VITA3K_BIN}" ]]; then
        echo "✅ Vita3K emulator installed at: ${VITA3K_BIN}"
    else
        echo "❌ Vita3K emulator not found at: ${VITA3K_BIN} (run: $0 --install-vita3k)"
    fi

    # 6. Check Sony Firmware in Vita3K
    local vita_fs="${HOME}/.local/share/Vita3K/Vita3K"
    if [[ -d "${vita_fs}/vs0" || -d "${vita_fs}/os0" ]]; then
        echo "✅ PS Vita official system firmware installed in Vita3K"
    else
        echo "❌ PS Vita firmware not detected in ${vita_fs} (run: $0 --install-firmware)"
    fi
}

install_just() {
    setup_user_env
    if command -v just >/dev/null 2>&1; then
        echo "✅ just is already installed: $(command -v just)"
        return 0
    fi
    echo "=== Installing just task runner to ${HOME}/.local/bin/just ==="
    mkdir -p "${HOME}/.local/bin"
    curl -sL "https://github.com/casey/just/releases/download/1.59.0/just-1.59.0-x86_64-unknown-linux-musl.tar.gz" | \
        tar -xz -C "${HOME}/.local/bin" just
    chmod +x "${HOME}/.local/bin/just"
    echo "✅ just installed successfully: $("${HOME}/.local/bin/just" --version)"
}

install_rust() {
    setup_user_env
    echo "=== Installing Rust Nightly and rust-src ==="
    rustup toolchain install nightly
    rustup component add rust-src --toolchain nightly
    echo "✅ Rust nightly & rust-src installed successfully."
}

install_vitasdk() {
    setup_user_env
    echo "=== Installing VitaSDK to ${VITASDK_DIR} ==="
    if [[ -x "${VITASDK_DIR}/bin/arm-vita-eabi-gcc" ]]; then
        echo "✅ VitaSDK already installed at: ${VITASDK_DIR}"
        return
    fi

    # Check write permissions or create directory
    if [[ ! -d "${VITASDK_DIR}" ]]; then
        if mkdir -p "${VITASDK_DIR}" 2>/dev/null; then
            :
        elif command -v sudo >/dev/null 2>&1; then
            sudo mkdir -p "${VITASDK_DIR}"
            sudo chown -R "$(whoami):$(whoami)" "${VITASDK_DIR}"
        else
            echo "Directory ${VITASDK_DIR} is not writable and sudo is not available."
            VITASDK_DIR="${HOME}/vitasdk"
            echo "Falling back to ${VITASDK_DIR}"
            mkdir -p "${VITASDK_DIR}"
        fi
    fi

    echo "Fetching latest VitaSDK Linux release URL..."
    local download_url
    download_url=$(curl -sL "https://api.github.com/repos/vitasdk/autobuilds/releases?per_page=30" 2>/dev/null | \
        grep -o '"browser_download_url": "[^"]*vitasdk-x86_64-linux-gnu-[^"]*\.tar\.bz2"' | \
        head -n 1 | cut -d '"' -f 4 || true)

    if [[ -z "${download_url}" ]]; then
        # Reliable fallback snapshot if GitHub API is rate-limited or offline
        download_url="https://github.com/vitasdk/autobuilds/releases/download/sdk-snapshot-20261015.850.1/vitasdk-x86_64-linux-gnu-2026-10-15_05-01-16.tar.bz2"
    fi

    echo "Downloading VitaSDK from: ${download_url}"
    local tmp_archive
    tmp_archive="$(mktemp /tmp/vitasdk_XXXXXX.tar.bz2)"
    curl -fL "${download_url}" -o "${tmp_archive}"
    tar -xj -C "${VITASDK_DIR}" --strip-components=1 -f "${tmp_archive}"
    rm -f "${tmp_archive}"

    echo "✅ VitaSDK installed to ${VITASDK_DIR}."
    echo ""
    echo "To add VitaSDK to your shell environment, add the following to ~/.bashrc or ~/.zshrc:"
    echo "  export VITASDK=\"${VITASDK_DIR}\""
    echo "  export PATH=\"\$VITASDK/bin:\$PATH\""
}

install_vita3k() {
    setup_user_env
    echo "=== Installing Vita3K to ${VITA3K_APPDIR} ==="
    mkdir -p "${VITA3K_APPDIR}" "$(dirname "${VITA3K_BIN}")"

    local appimage_tmp
    appimage_tmp="$(mktemp /tmp/Vita3K_XXXXXX.AppImage)"
    echo "Downloading Vita3K continuous AppImage..."
    curl -fL "https://github.com/Vita3K/Vita3K/releases/download/continuous/Vita3K-x86_64.AppImage" -o "${appimage_tmp}"
    chmod +x "${appimage_tmp}"

    echo "Extracting Vita3K AppImage..."
    rm -rf "${VITA3K_APPDIR}"
    (cd "$(dirname "${VITA3K_APPDIR}")" && "${appimage_tmp}" --appimage-extract >/dev/null 2>&1)
    mv "$(dirname "${VITA3K_APPDIR}")/squashfs-root" "${VITA3K_APPDIR}" 2>/dev/null || \
        mv "$(dirname "${VITA3K_APPDIR}")/AppDir" "${VITA3K_APPDIR}"
    rm -f "${appimage_tmp}"

    echo "Creating robust Vita3K launcher script at ${VITA3K_BIN}..."
    rm -f "${VITA3K_BIN}"
    cat << 'EOF' > "${VITA3K_BIN}"
#!/usr/bin/env bash
set -e

# Ensure valid runtime directory
if [[ -z "${XDG_RUNTIME_DIR:-}" || ! -d "${XDG_RUNTIME_DIR}" || ! -w "${XDG_RUNTIME_DIR}" ]]; then
    export XDG_RUNTIME_DIR="/tmp/runtime-$(id -u)"
    mkdir -p "${XDG_RUNTIME_DIR}"
    chmod 700 "${XDG_RUNTIME_DIR}"
fi

export QT_QPA_PLATFORM=xcb
export SDL_VIDEODRIVER=x11
export LIBGL_ALWAYS_SOFTWARE=1
export MESA_GL_VERSION_OVERRIDE=4.5
export MESA_GLSL_VERSION_OVERRIDE=450

VITA3K_APPDIR="${HOME}/.local/share/vita3k-appdir"

# Default to OpenGL renderer for maximum compatibility across headless/Mesa environments
HAS_BACKEND=false
for arg in "$@"; do
    if [[ "$arg" == "-B" || "$arg" == "--backend-renderer" ]]; then
        HAS_BACKEND=true
        break
    fi
done

if [[ -f "${VITA3K_APPDIR}/sharun" ]]; then
    if [[ "$HAS_BACKEND" == "true" ]]; then
        exec "${VITA3K_APPDIR}/sharun" "${VITA3K_APPDIR}/shared/bin/Vita3K" "$@"
    else
        exec "${VITA3K_APPDIR}/sharun" "${VITA3K_APPDIR}/shared/bin/Vita3K" -B OpenGL "$@"
    fi
else
    if [[ "$HAS_BACKEND" == "true" ]]; then
        exec "${VITA3K_APPDIR}/bin/Vita3K" "$@"
    else
        exec "${VITA3K_APPDIR}/bin/Vita3K" -B OpenGL "$@"
    fi
fi
EOF
    chmod +x "${VITA3K_BIN}"

    # Configure Vita3K config.yml to skip welcome screens and wizard in headless/CLI runs
    mkdir -p "${HOME}/.config/Vita3K"
    local cfg="${HOME}/.config/Vita3K/config.yml"
    if [[ -f "${cfg}" ]]; then
        sed -i 's/initial-setup: false/initial-setup: true/' "${cfg}" || true
        sed -i 's/show-welcome: true/show-welcome: false/' "${cfg}" || true
        sed -i 's/warn-missing-firmware: true/warn-missing-firmware: false/' "${cfg}" || true
    else
        cat << 'EOF' > "${cfg}"
---
initial-setup: true
show-welcome: false
warn-missing-firmware: false
backend-renderer: OpenGL
pref-path: /home/antigravity/.local/share/Vita3K/Vita3K
...
EOF
    fi

    echo "✅ Vita3K installed successfully: $("${VITA3K_BIN}" --version)"
}

install_firmware() {
    setup_user_env
    if [[ ! -x "${VITA3K_BIN}" ]]; then
        echo "❌ Vita3K binary not found at ${VITA3K_BIN}. Run: $0 --install-vita3k first." >&2
        exit 1
    fi

    echo "=== Downloading & Installing Sony PS Vita Firmware into Vita3K ==="
    mkdir -p /tmp/vita-fw
    echo "Downloading official PS Vita Firmware (PSP2UPDAT.PUP)..."
    curl -fL "http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP" \
        -o /tmp/vita-fw/PSP2UPDAT.PUP

    echo "Installing main system firmware into Vita3K..."
    "${VITA3K_BIN}" --console 1 --firmware /tmp/vita-fw/PSP2UPDAT.PUP

    echo "Attempting font package download (optional)..."
    if curl -s -fL "http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP" \
        -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP 2>/dev/null; then
        echo "Installing font package..."
        "${VITA3K_BIN}" --console 1 --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP || true
    else
        echo "ℹ️  Font package not available on Sony CDN (homebrew app uses embedded Poppins fonts; skipping)."
    fi

    rm -rf /tmp/vita-fw
    echo "✅ Firmware installation complete."
}

install_all() {
    echo "=== Running Full PS Vita Development Environment Setup ==="
    install_just
    install_rust
    install_vitasdk
    install_vita3k
    install_firmware
    echo ""
    echo "=========================================================="
    echo "🎉 Full PS Vita Development Environment setup complete!"
    echo "Run './scripts/setup_env.sh --check-all' to verify tools."
    echo "=========================================================="
}

ACTION="${1:---check-all}"
case "$ACTION" in
    --check-all)
        check_tools
        ;;
    --install-all)
        install_all
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
    --install-rust)
        install_rust
        ;;
    --install-just)
        install_just
        ;;
    *)
        echo "Usage: $0 [--check-all | --install-all | --install-vitasdk | --install-vita3k | --install-firmware | --install-rust | --install-just]"
        exit 1
        ;;
esac
