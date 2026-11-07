---
name: vita-crash-dump-analyzer
description: >-
  Use this skill when the user reports a PlayStation Vita crash error (such as
  C2-12828-1) or provides a core dump filename like
  ux0:data/psp2core-<timestamp>-0x<offset>-eboot.bin.psp2dmp.
  Disassembles the exact faulting instruction in vita-syncthing-app.elf and
  identifies hardware root causes.
---

# PS Vita Crash Dump (`C2-12828-1` / `psp2core`) Analyzer Skill

When a real PlayStation Vita crashes with `(C2-12828-1)`, `SceProcessmgr` writes a core dump filename of the form:
```text
ux0:data/psp2core-<timestamp>-0x<hex_offset>-eboot.bin.psp2dmp
```
The hex value `0x<hex_offset>` (with bit 0 cleared for Thumb instructions) is the relative offset from the ELF `.text` base address (`0x81000000`).

## Steps

1. **Disassemble the Faulting Address**:
   Run [analyze_crash_offset.sh](./scripts/analyze_crash_offset.sh) with either the hex offset (`0x00000d3b6d`) or the full `psp2core-...` filename:
   ```bash
   ./.agents/skills/vita-crash-dump-analyzer/scripts/analyze_crash_offset.sh 0x00000d3b6d
   ```

2. **Check Common Real-Hardware Failure Patterns**:
   - **Fault inside `std::sys::stdio::unix::Stderr::write_all` / `abort` / `malloc`**:
     - Indicates a Rust panic or out-of-memory abort during early startup or large allocation.
     - Verify `_newlib_heap_size_user` is exported (`64 * 1024 * 1024`) and not weak (`w`).
   - **Fault inside `sceNet*` or `sceNetCtl*` stubs**:
     - Verify `sceSysmoduleLoadModule(SCE_SYSMODULE_NET)` (`0x0001`) is called before `sceNetInit` and `sceNetCtlInit`.
     - Verify `SceNetInitParam.memory` is allocated via `sceKernelAllocMemBlock("SceNetMemory", 0x0C20D060, ...)`.
   - **Fault on function prologue (`stmdb sp!, ...` / `sub.w sp, sp, #...`)**:
     - Indicates stack overflow on the main thread or `bep_worker` thread.
     - Verify `sceUserMainThreadStackSize` (`2 * 1024 * 1024`) and `Builder::new().stack_size(2 * 1024 * 1024)`, and keep large structs (`VitaDashboard`, buffers >= 64 KiB) heap-allocated (`Box` / `Vec`).
