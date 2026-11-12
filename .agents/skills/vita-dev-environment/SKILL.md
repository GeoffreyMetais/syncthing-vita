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
 ├── VitaSDK Toolchain (`$VITASDK` -> `/usr/local/vitasdk` or `/opt/vitasdk`)
 │    ├── Compiler & Linker: arm-vita-eabi-gcc, arm-vita-eabi-g++, arm-vita-eabi-ld, arm-vita-eabi-nm
 │    ├── Homebrew Packaging: vita-elf-create, vita-make-fself, vita-mksfoex, vita-pack-vpk
 │    └── Package Manager: vdpm (Vita Development Package Manager)
 ├── Emulator: Vita3K (`~/.local/bin/Vita3K`) + Official Sony Firmware (`PSP2UPDAT.PUP`)
 └── Deployment: VitaShell FTP (port 1337) / USB MTP
```

---

## 2. VitaSDK Installation

VitaSDK is the standard open-source cross-compilation toolchain for the PlayStation Vita (ARM Cortex-A9).

### Automated Installation (Linux / macOS / WSL)

1. **Set Environment Variables**:
   Add to `~/.bashrc`, `~/.zshrc`, or project environment scripts:
   ```bash
   export VITASDK=/usr/local/vitasdk
   export PATH="$VITASDK/bin:$PATH"
   ```

2. **Install VitaSDK with the official bootstrap**:
   The old `vitasdk-x86_64-linux-ubuntu-latest.tar.bz2` autobuild asset no longer exists.
   The `vdpm` bootstrap installs the newest supported series as pacman packages and verifies
   the signed channel manifest (it needs `github.com` and `vitasdk.org`, and refuses an
   existing destination directory):
   ```bash
   git clone --depth 1 https://github.com/vitasdk/vdpm /tmp/vdpm
   (cd /tmp/vdpm && ./bootstrap-vitasdk.sh)
   ```
   `scripts/setup_env.sh --install-vitasdk` does the same.

3. **Install Core Libraries via `vdpm`**:
   VitaSDK includes `vdpm` (Vita Development Package Manager) to fetch pre-compiled libraries:
   ```bash
   vdpm zlib libpng libjpeg-turbo
   ```

4. **Verify Installation**:
   ```bash
   arm-vita-eabi-gcc --version
   vita-elf-create -v
   vita-make-fself -v
   ```

---

## 3. Rust Cross-Compilation Setup (`armv7-sony-vita-newlibeabihf`)

Rust has tier-3 support for the PS Vita target (`armv7-sony-vita-newlibeabihf`).

### Prerequisites
1. **Rust Nightly Toolchain**:
   ```bash
   rustup toolchain install nightly
   rustup component add rust-src --toolchain nightly
   ```

2. **Cargo-Vita Tool (Optional Helper)**:
   ```bash
   cargo install cargo-vita
   ```

### Cargo Compilation Command
When compiling Rust crates for the PS Vita without `cargo-vita`:
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

### Step-by-Step Manual Pipeline
```bash
# 1. Convert standard ELF to Vita ELF (resolves NIDs and relocation stubs)
vita-elf-create target/armv7-sony-vita-newlibeabihf/release/my-app.elf target/my-app.velf

# 2. Build eboot.bin
# Use -c for compressed homebrew eboot (requires full permissions for ux0: / sysmodules)
vita-make-fself -c target/my-app.velf pkg/eboot.bin

# 3. Create param.sfo
# ATTRIBUTE2=12 unlocks extended memory (+109 MB RAM)
mkdir -p pkg/sce_sys
vita-mksfoex -s TITLE_ID="MYAPP0001" -d ATTRIBUTE2=12 "My Application" pkg/sce_sys/param.sfo

# 4. Package VPK
vita-pack-vpk -s pkg/sce_sys/param.sfo -b pkg/eboot.bin target/MYAPP0001.vpk
```

---

## 5. Vita3K Emulator Installation & Setup

Vita3K is the leading experimental open-source PlayStation Vita emulator for Windows, Linux, and macOS.

### Installation on Linux (x86_64)

1. **Install Dependencies**:
   ```bash
   sudo apt-get install libsdl2-2.0-0 libcurl4 libssl3 libvulkan1
   ```

2. **Download Vita3K Binary or AppImage**:
   ```bash
   mkdir -p ~/.local/bin
   curl -fL https://github.com/Vita3K/Vita3K/releases/download/continuous/Vita3K-x86_64.AppImage -o ~/.local/bin/Vita3K
   chmod +x ~/.local/bin/Vita3K
   ```
   In containers without FUSE, extract the AppImage instead (`scripts/setup_env.sh --install-vita3k`
   installs an extracted copy behind a `~/.local/bin/Vita3K` wrapper).

3. **Install Sony Official System Firmware & Font Package**:
   Vita3K requires both Sony firmware `.PUP` files to render text and system assets:
   ```bash
   mkdir -p /tmp/vita-fw
   # Main Firmware PUP
   curl -fL http://dus01.psp2.update.playstation.net/update/psp2/image/2022_0209/rel_f2c7b12fe85496ec88a0391b514d6e3b/PSP2UPDAT.PUP -o /tmp/vita-fw/PSP2UPDAT.PUP
   # Font Package PUP
   curl -fL http://dus01.psv.update.playstation.net/update/psv/image/2012_0208/sd_2f1289b2d864758a09225d2789fa3996/PSP2UPDAT.PUP -o /tmp/vita-fw/FONT_PSP2UPDAT.PUP

   # Install into Vita3K
   ~/.local/bin/Vita3K -z --firmware /tmp/vita-fw/PSP2UPDAT.PUP
   ~/.local/bin/Vita3K -z --firmware /tmp/vita-fw/FONT_PSP2UPDAT.PUP
   rm -rf /tmp/vita-fw
   ```

4. **Directory Structure**:
   On Linux, Vita3K emulates storage under:
   `~/.local/share/Vita3K/Vita3K/ux0/`
   Installed homebrew applications live in:
   `~/.local/share/Vita3K/Vita3K/ux0/app/<TITLE_ID>/`

5. **Headless / Console Execution**:
   To run an app and monitor console output from stdout:
   ```bash
   ~/.local/bin/Vita3K -z --frontend sdl -l 2 -r <TITLE_ID>
   ```
   Recent Vita3K builds take `-z` / `--console` **without a value**: `--console 1` makes Vita3K
   treat `1` as a content path.
   Headless (no GPU, e.g. cloud containers): run under Xvfb with Mesa's software OpenGL:
   ```bash
   mkdir -p -m 700 /tmp/xdg
   XDG_RUNTIME_DIR=/tmp/xdg LIBGL_ALWAYS_SOFTWARE=1 MESA_GL_VERSION_OVERRIDE=4.6 MESA_GLSL_VERSION_OVERRIDE=460 \
     xvfb-run -a -s "-screen 0 1280x720x24" ~/.local/bin/Vita3K -z --frontend sdl -B OpenGL -l 2 -r <TITLE_ID>
   ```
   The Sony firmware is optional for homebrew. Its update servers only serve plain HTTP, so the
   download fails behind HTTPS-only proxies.

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
