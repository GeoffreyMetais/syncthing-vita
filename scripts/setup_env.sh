#!/usr/bin/env bash
set -euo pipefail

# setup_env.sh: Automates VitaSDK, Rust cross-compilation, and Vita3K emulator setup.
#
# Usage:
#   ./scripts/setup_env.sh --check-all
#   ./scripts/setup_env.sh --install-all
#   ./scripts/setup_env.sh --install-vitasdk
#   ./scripts/setup_env.sh --install-vita3k
#   ./scripts/setup_env.sh --prepare-vita3k
#   ./scripts/setup_env.sh --install-firmware   # optional: the app runs in Vita3K without it
#   ./scripts/setup_env.sh --install-rust
#   ./scripts/setup_env.sh --install-just
#   ./scripts/setup_env.sh --install-audit

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
VITA3K_PREF="${HOME}/.local/share/Vita3K/Vita3K"

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

    if cargo audit --version >/dev/null 2>&1; then
        echo "✅ cargo-audit: present ($(cargo audit --version))"
    else
        echo "❌ cargo-audit: missing (run: $0 --install-audit)"
    fi

    # 5. Check Vita3K Emulator
    if [[ -x "${VITA3K_BIN}" ]]; then
        echo "✅ Vita3K emulator installed at: ${VITA3K_BIN}"
    else
        echo "❌ Vita3K emulator not found at: ${VITA3K_BIN} (run: $0 --install-vita3k)"
    fi

    # 6. Check Vita3K storage (ux0:app receives the VPK; the run scripts create it too)
    if [[ -d "${VITA3K_PREF}/ux0/app" ]]; then
        echo "✅ Vita3K storage ready: ${VITA3K_PREF}/ux0"
    else
        echo "ℹ️  Vita3K storage not created yet: ${VITA3K_PREF}/ux0 (run: $0 --prepare-vita3k, or created on first run)"
    fi

    # 7. Display: Vita3K can run on X11, Wayland, or headless via xvfb-run
    if [[ -n "${DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]]; then
        echo "✅ Graphical display available: ${DISPLAY:-$WAYLAND_DISPLAY}"
    elif command -v xvfb-run >/dev/null 2>&1; then
        echo "✅ No display: Vita3K runs headless via xvfb-run (scripts/vita3k_launch.sh)"
    else
        echo "ℹ️  No active display in current shell (scripts/vita3k_launch.sh will auto-detect desktop session, or install xvfb)"
    fi

    # 8. Sony firmware: optional (the app only uses HLE modules and embeds its fonts)
    # Vita3K creates empty partition folders on first start; installed firmware fills vs0/sys/external.
    if [[ -n "$(ls -A "${VITA3K_PREF}/vs0/sys/external" 2>/dev/null)" ]]; then
        echo "✅ PS Vita system firmware installed in Vita3K (optional)"
    else
        echo "ℹ️  PS Vita firmware not installed (optional, not needed to run the app: $0 --install-firmware)"
    fi
}

# Writes a headless-friendly Vita3K config and creates the emulated ux0: storage.
prepare_vita3k() {
    mkdir -p "${HOME}/.config/Vita3K" "${VITA3K_PREF}/ux0/app" "${VITA3K_PREF}/ux0/data"
    local cfg="${HOME}/.config/Vita3K/config.yml"
    if [[ -f "${cfg}" ]]; then
        sed -i 's/^initial-setup: false/initial-setup: true/' "${cfg}" || true
        sed -i 's/^show-welcome: true/show-welcome: false/' "${cfg}" || true
        sed -i 's/^warn-missing-firmware: true/warn-missing-firmware: false/' "${cfg}" || true
        sed -i "s|^pref-path: .*|pref-path: ${VITA3K_PREF}|" "${cfg}" || true
    else
        cat << EOF > "${cfg}"
---
initial-setup: true
show-welcome: false
warn-missing-firmware: false
backend-renderer: OpenGL
pref-path: ${VITA3K_PREF}
...
EOF
    fi
    echo "✅ Vita3K configured (storage: ${VITA3K_PREF}/ux0, no firmware required)"
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
    rustup toolchain install nightly --no-self-update
    rustup component add rust-src --toolchain nightly
    if ! rustup show active-toolchain >/dev/null 2>&1; then
        rustup default nightly
    fi
    echo "✅ Rust nightly & rust-src installed successfully."
}

install_audit() {
    setup_user_env
    if cargo audit --version >/dev/null 2>&1; then
        echo "✅ cargo-audit is already installed: $(cargo audit --version)"
        return 0
    fi
    echo "=== Installing cargo-audit ==="
    cargo install cargo-audit --locked
    echo "✅ cargo-audit installed successfully: $(cargo audit --version)"
}

resolve_vitasdk_release() {
    local channels_base="https://vitasdk.org/channels"
    local index_json
    index_json="$(curl -fsSL --max-time 15 "${channels_base}/index.json" 2>/dev/null || true)"
    [[ -n "${index_json}" ]] || return 1

    local channel
    channel="$(printf '%s' "${index_json}" | python3 -c '
import json, sys
data = json.load(sys.stdin)
supported = [
    name for name, meta in data.get("channels", {}).items()
    if meta.get("status") == "supported" and meta.get("world", "vita") == "vita"
]
def key(name):
    return [int(p) if p.isdigit() else 0 for p in name.split(".")]
if supported:
    print(sorted(supported, key=key, reverse=True)[0])
' 2>/dev/null || true)"
    [[ -n "${channel}" && "${channel}" =~ ^[A-Za-z0-9._-]+$ ]] || return 1

    local channel_json
    channel_json="$(curl -fsSL --max-time 15 "${channels_base}/${channel}.json" 2>/dev/null || true)"
    [[ -n "${channel_json}" ]] || return 1

    printf '%s' "${channel_json}" | python3 -c '
import json, sys
data = json.load(sys.stdin)
core = data.get("core", {})
repo = core.get("repository", "")
rel = core.get("release", "")
if repo and rel:
    print(f"{repo}|{rel}")
' 2>/dev/null
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

    echo "Resolving supported VitaSDK release channel..."
    local resolved repo="" release="" expected_sha256=""
    resolved="$(resolve_vitasdk_release || true)"
    if [[ "${resolved}" == *"|"* ]]; then
        repo="${resolved%%|*}"
        release="${resolved#*|}"
    fi
    if [[ ! "${repo}" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ || ! "${release}" =~ ^[A-Za-z0-9._-]+$ ]]; then
        # Reliable fallback to the pinned supported 2026.08 release
        repo="vitasdk/autobuilds"
        release="sdk-snapshot-20260825.611.1"
        expected_sha256="3d5be8ad0fae5de872c13328f3508a59b9e1eec504269eec64e47d06de78b098"
    fi

    local download_url="https://github.com/${repo}/releases/download/${release}/vitasdk-bootstrap-x86_64-linux-gnu.tar.bz2"
    if [[ -z "${expected_sha256}" ]]; then
        expected_sha256="$(curl -fsSL --max-time 15 "${download_url}.sha256" 2>/dev/null | awk '{print tolower($1); exit}' || true)"
    fi

    echo "Downloading VitaSDK (${release}) from: ${download_url}"
    local tmp_archive
    tmp_archive="$(mktemp /tmp/vitasdk_XXXXXX.tar.bz2)"
    if ! curl -fL "${download_url}" -o "${tmp_archive}"; then
        rm -f "${tmp_archive}"
        return 1
    fi

    if command -v sha256sum >/dev/null 2>&1 && [[ "${expected_sha256}" =~ ^[0-9a-f]{64}$ ]]; then
        local actual_sha256
        actual_sha256="$(sha256sum "${tmp_archive}" | awk '{print tolower($1)}')"
        if [[ "${actual_sha256}" != "${expected_sha256}" ]]; then
            echo "❌ VitaSDK SHA-256 mismatch (expected ${expected_sha256}, got ${actual_sha256})." >&2
            rm -f "${tmp_archive}"
            return 1
        fi
        echo "✅ Verified VitaSDK SHA-256: ${actual_sha256}"
    fi

    if ! tar -xj -C "${VITASDK_DIR}" --strip-components=1 -f "${tmp_archive}"; then
        rm -f "${tmp_archive}"
        return 1
    fi
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
    local share_dir
    share_dir="$(dirname "${VITA3K_APPDIR}")"
    rm -rf "${VITA3K_APPDIR}" "${share_dir}/squashfs-root" "${share_dir}/AppDir"
    (cd "${share_dir}" && "${appimage_tmp}" --appimage-extract >/dev/null 2>&1)
    if [[ -d "${share_dir}/AppDir" && ! -L "${share_dir}/AppDir" ]]; then
        rm -f "${share_dir}/squashfs-root"
        mv "${share_dir}/AppDir" "${VITA3K_APPDIR}"
    else
        mv "${share_dir}/squashfs-root" "${VITA3K_APPDIR}"
    fi
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

if [[ -x "${VITA3K_APPDIR}/AppRun" ]]; then
    if [[ "$HAS_BACKEND" == "true" ]]; then
        exec "${VITA3K_APPDIR}/AppRun" "$@"
    else
        exec "${VITA3K_APPDIR}/AppRun" -B OpenGL "$@"
    fi
elif [[ -f "${VITA3K_APPDIR}/sharun" ]]; then
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

    # Skip welcome screens / firmware warnings in headless runs and create ux0: storage
    prepare_vita3k

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
    "${VITA3K_BIN}" -z --firmware /tmp/vita-fw/PSP2UPDAT.PUP

    echo "Attempting font package download (optional)..."
    if curl -s -fL "http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP" \
        -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP 2>/dev/null; then
        echo "Installing font package..."
        "${VITA3K_BIN}" -z --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP || true
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
    install_audit
    install_vitasdk
    install_vita3k
    echo "ℹ️  Sony firmware skipped: not needed to run the app (optional: $0 --install-firmware)"
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
    --prepare-vita3k)
        prepare_vita3k
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
    --install-audit)
        install_audit
        ;;
    *)
        echo "Usage: $0 [--check-all | --install-all | --install-vitasdk | --install-vita3k | --prepare-vita3k | --install-firmware | --install-rust | --install-just | --install-audit]"
        exit 1
        ;;
esac
