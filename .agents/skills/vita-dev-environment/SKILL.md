---
name: vita-dev-environment
description: >-
  PlayStation Vita development environment setup and toolchain reference. Use this
  skill when setting up, verifying, or troubleshooting VitaSDK, cargo-vita / Rust cross-compilation
  for armv7-sony-vita-newlibeabihf, Vita3K emulator installation & firmware setup, or
  homebrew packaging tooling.
---

# PlayStation Vita Development Environment Guide

This skill provides comprehensive instructions for installing, configuring, and verifying the PlayStation Vita homebrew development toolchain, Rust cross-compilation pipeline, and Vita3K emulator.

---

## 1. Toolchain Architecture Overview

```text
Host System (Linux / macOS / WSL2)
 ├── Rust Nightly (`armv7-sony-vita-newlibeabihf` target via `-Z build-std=std,panic_abort`)
 ├── VitaSDK Toolchain (`$VITASDK` -> `/usr/local/vitasdk` or `$HOME/vitasdk`)
 │    ├── Compiler & Linker: arm-vita-eabi-gcc, arm-vita-eabi-g++, arm-vita-eabi-ld, arm-vita-eabi-nm
 │    ├── Homebrew Packaging: vita-elf-create, vita-make-fself, vita-mksfoex, vita-pack-vpk
 │    └── Package Manager: vdpm (Vita Development Package Manager)
 ├── Emulator: Vita3K (`~/.local/bin/Vita3K`) + Official Sony Firmware (`PSP2UPDAT.PUP`)
 └── Deployment: VitaShell FTP (port 1337) / USB MTP
```

---

## 2. VitaSDK Installation

VitaSDK is the standard open-source cross-compilation toolchain for the PlayStation Vita (ARM Cortex-A9).

### Automated Setup (Recommended)

Run the project setup script or `just`:
```bash
# Verify current environment status
just check-env
# or: ./scripts/setup_env.sh --check-all

# Automatically install all missing components (Rust nightly, Just, VitaSDK, Vita3K, Firmware)
just setup
# or: ./scripts/setup_env.sh --install-all
```

### Manual VitaSDK Installation (Linux / macOS / WSL)

1. **Set Environment Variables**:
   Add to `~/.bashrc`, `~/.zshrc`, or project environment scripts:
   ```bash
   export VITASDK="${VITASDK:-$HOME/vitasdk}"
   export PATH="$VITASDK/bin:$PATH"
   ```

2. **Download and Extract Prebuilt VitaSDK**:
   VitaSDK autobuild releases are tagged as `sdk-snapshot-<date>.<build>.<rev>`. Fetch the latest snapshot dynamically or use the `vdpm` bootstrap:
   ```bash
   mkdir -p "$VITASDK"
   # Dynamically resolve latest Linux snapshot tarball
   DOWNLOAD_URL=$(curl -sL "https://api.github.com/repos/vitasdk/autobuilds/releases?per_page=30" | \
       grep -o '"browser_download_url": "[^"]*vitasdk-x86_64-linux-gnu-[^"]*\.tar\.bz2"' | \
       head -n 1 | cut -d '"' -f 4)

   # Fallback snapshot if GitHub API is offline or rate-limited:
   DOWNLOAD_URL="${DOWNLOAD_URL:-https://github.com/vitasdk/autobuilds/releases/download/sdk-snapshot-20261015.850.1/vitasdk-x86_64-linux-gnu-2026-10-15_05-01-16.tar.bz2}"

   curl -fL "$DOWNLOAD_URL" | tar -xj -C "$VITASDK" --strip-components=1
   ```
   *(Alternatively, run `git clone --depth 1 https://github.com/vitasdk/vdpm /tmp/vdpm && (cd /tmp/vdpm && ./bootstrap-vitasdk.sh)`)*

3. **Verify Installation**:
   ```bash
   arm-vita-eabi-gcc --version
   vita-elf-create -v
   vita-make-fself -v
   vita-mksfoex -v
   vita-pack-vpk -v
   ```

---

## 3. Rust Cross-Compilation Setup (`armv7-sony-vita-newlibeabihf`)

Rust has tier-3 support for the PS Vita target (`armv7-sony-vita-newlibeabihf`).

### Prerequisites
1. **Rust Nightly Toolchain & Component**:
   ```bash
   # In unprivileged/container environments where /usr/local/rustup is read-only:
   export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
   export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
   export PATH="$CARGO_HOME/bin:$PATH"

   rustup toolchain install nightly
   rustup component add rust-src --toolchain nightly
   ```

2. **Just Task Runner**:
   ```bash
   ./scripts/setup_env.sh --install-just
   ```

### Cargo Compilation Command
When compiling Rust crates for the PS Vita:
```bash
cargo +nightly build \
    -Z build-std=std,panic_abort \
    --target armv7-sony-vita-newlibeabihf \
    --release
```

### Critical Target Configuration (`.cargo/config.toml` or Environment)
Ensure the cross-linker is pointed to VitaSDK:
```toml
[target.armv7-sony-vita-newlibeabihf]
linker = "arm-vita-eabi-gcc"
runner = "vita3k"

[build]
target = "armv7-sony-vita-newlibeabihf"
```

---

## 4. Packaging Pipeline (`ELF` -> `.vpk`)

A PS Vita `.vpk` is a standard ZIP archive containing:
- `eboot.bin`: Self-decrypting PlayStation Vita executable (signed/encrypted fself).
- `sce_sys/param.sfo`: Metadata file (Title ID, Title Name, Category, Permissions, Extended Memory).
- Assets: `sce_sys/icon0.png` (128x128), `sce_sys/livearea/contents/bg.png` (840x500), fonts, sounds.

### Automated Packaging
```bash
just vpk
# Or: ./scripts/build_vpk.sh
```

### Verification & Symbol Audit
```bash
just verify
# Or: ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh
```

---

## 5. Vita3K Emulator Installation & Setup

Vita3K is the leading experimental open-source PlayStation Vita emulator for Windows, Linux, and macOS.

### Automated Installation via Setup Script
```bash
./scripts/setup_env.sh --install-vita3k
./scripts/setup_env.sh --install-firmware
```

This:
1. Extracts the AppImage to `~/.local/share/vita3k-appdir` (avoiding `/dev/fuse` requirements in containers/WSL).
2. Sets up a bundled runtime launcher at `~/.local/bin/Vita3K` using `sharun` to isolate Qt6 and glibc dependencies.
3. Automatically sets OpenGL overrides (`MESA_GL_VERSION_OVERRIDE=4.5`, `MESA_GLSL_VERSION_OVERRIDE=450`, `LIBGL_ALWAYS_SOFTWARE=1`) for headless Xvfb or software rasterizers.
4. Preconfigures `~/.config/Vita3K/config.yml` with `initial-setup: true` and `show-welcome: false` so tests run headlessly without wizard screens.

### Manual Firmware Installation:
```bash
mkdir -p /tmp/vita-fw
# Main Firmware PUP (127 MB, required)
curl -fL http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP -o /tmp/vita-fw/PSP2UPDAT.PUP
~/.local/bin/Vita3K -z --firmware /tmp/vita-fw/PSP2UPDAT.PUP
rm -rf /tmp/vita-fw
```
*(Note: Sony firmware servers serve plain HTTP; downloads may fail behind HTTPS-only proxies. VitaSyncthing bundles its own SIL OFL Poppins TrueType fonts, so Sony system fonts are not required).*

### Directory Structure:
On Linux, Vita3K emulates storage under:
`~/.local/share/Vita3K/Vita3K/ux0/`
Installed homebrew applications live in:
`~/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING/`

### Headless / Console Execution:
```bash
just run-vita3k
```
Recent Vita3K builds take `-z` / `--console` without a value (`--console 1` makes Vita3K treat `1` as a content path).

For headless environments without a physical GPU:
```bash
mkdir -p -m 700 /tmp/xdg
XDG_RUNTIME_DIR=/tmp/xdg LIBGL_ALWAYS_SOFTWARE=1 MESA_GL_VERSION_OVERRIDE=4.5 MESA_GLSL_VERSION_OVERRIDE=450 \
  xvfb-run -a -s "-screen 0 1024x768x24" ~/.local/bin/Vita3K -z --frontend sdl -B OpenGL -l 2 -r SYNCTHING
```

---

## 6. Real PS Vita Hardware Constraints & Rules

Code that works in Vita3K can still crash on real PS Vita hardware (`C2-12828-1`). Always enforce:

1. **Newlib Heap Size (`_newlib_heap_size_user`)**:
   VitaSDK's libc defaults to allocating 128 MB in a single contiguous block, which fails on real hardware. Export:
   ```rust
   #[no_mangle]
   pub static _newlib_heap_size_user: u32 = 64 * 1024 * 1024; // 64 MB
   ```

2. **Main Thread Stack Size**:
   Default stack is only 256 KB. For network or complex UI apps, export:
   ```rust
   #[no_mangle]
   pub static sceUserMainThreadStackSize: u32 = 2 * 1024 * 1024; // 2 MB
   ```

3. **Sysmodule Loading (`SCE_SYSMODULE_NET`)**:
   Always load sysmodules via `sceSysmoduleLoadModule` before initializing subsystems (e.g. `sceNetInit`).

4. **FSELF Compression**:
   Always use `vita-make-fself -c` rather than `-s`.
