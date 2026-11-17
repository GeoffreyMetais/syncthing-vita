---
name: vita-vpk-verify
description: >-
  Use this skill when building, verifying, smoke-testing in Vita3K, or deploying
  the VitaSyncthing release package (SYNCTHING.vpk) to a real PlayStation Vita.
  Verifies hardware-critical ELF symbols (_newlib_heap_size_user, stack size)
  to prevent C2-12828-1 crashes on real PS Vita hardware.
---

# PS Vita VPK Build, Hardware Audit & Deployment Skill

Follow this workflow whenever producing a release build of `vita-syncthing-app` (`SYNCTHING.vpk`) or testing changes in Vita3K / real PS Vita hardware.

## Steps

1. **Run the automated verification & build script**:
   Execute [verify_and_deploy.sh](./scripts/verify_and_deploy.sh) from the repository root:
   ```bash
   ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh
   ```
   This script performs:
   - Host unit tests (`cargo test --workspace`)
   - Release `.vpk` compilation (`just vpk`)
   - ELF symbol audit via `arm-vita-eabi-nm` to confirm `_newlib_heap_size_user` (64 MB) and `sceUserMainThreadStackSize` (2 MB) are exported in `.rodata` and bound inside `_init_vita_heap`
   - Background sync plugin audit (`scripts/audit_plugin.sh`): `plugins/syncthing_launcher.suprx` and `plugins/syncthing_worker.suprx` are bundled in the VPK, both export `module_start` / `module_stop`, the launcher links no heap, the worker's `_newlib_heap_size_user` is 8 MiB and its `abort` / `_free_vita_newlib` overrides are linked (a fatal error ends the worker thread, never `SceShell`). This is static only: Vita3K cannot load taiHEN plugins into `SceShell`, so the plugin is only verified on real hardware (AGENTS.md §8)
   - Automatic unpacking into Vita3K (`~/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING`)

2. **Optional: Smoke-test in Vita3K** (no Sony firmware needed; `vita3k_launch.sh` uses `xvfb-run` when there is no display):
   ```bash
   timeout 6s ./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING || true; pkill -x Vita3K || true
   tail -n 20 ~/.local/share/Vita3K/Vita3K/ux0/data/syncthing/syncthing.log
   ```

3. **Optional: Wireless FTP Upload to a Real PlayStation Vita**:
   If the user provides their PS Vita's IP address with VitaShell FTP active (port `1337`):
   ```bash
   ./.agents/skills/vita-vpk-verify/scripts/verify_and_deploy.sh --ftp <VITA_IP>:1337
   ```
