---
name: vita-vpk-verify
description: >-
  Use this skill when building, verifying, smoke-testing in Vita3K, or deploying
  the VitaSyncthing release package (SYNC00001.vpk) to a real PlayStation Vita.
  Verifies hardware-critical ELF symbols (_newlib_heap_size_user, stack size)
  to prevent C2-12828-1 crashes on real PS Vita hardware.
---

# PS Vita VPK Build, Hardware Audit & Deployment Skill

Follow this workflow whenever producing a release build of `vita-syncthing-app` (`SYNC00001.vpk`) or testing changes in Vita3K / real PS Vita hardware.

## Steps

1. **Run the automated verification & build script**:
   Execute [verify_and_deploy.sh](./scripts/verify_and_deploy.sh) from `/mnt/projects/vita-dev`:
   ```bash
   ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh
   ```
   This script performs:
   - Host unit tests (`cargo test --workspace`)
   - Release `.vpk` compilation (`just vpk`)
   - ELF symbol audit via `arm-vita-eabi-nm` to confirm `_newlib_heap_size_user` (64 MB) and `sceUserMainThreadStackSize` (2 MB) are exported in `.rodata` and bound inside `_init_vita_heap`
   - Automatic unpacking into Vita3K (`~/.local/share/Vita3K/Vita3K/ux0/app/SYNC00001`)

2. **Optional: Smoke-test in Vita3K**:
   ```bash
   timeout 6s ~/.local/bin/Vita3K --console 1 --frontend sdl -l 2 -r SYNC00001 || true
   tail -n 20 ~/.local/share/Vita3K/Vita3K/ux0/data/syncthing/syncthing.log
   ```

3. **Optional: Wireless FTP Upload to a Real PlayStation Vita**:
   If the user provides their PS Vita's IP address with VitaShell FTP active (port `1337`):
   ```bash
   ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh --ftp <VITA_IP>:1337
   ```
