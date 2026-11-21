---
name: rust-quality-checks
description: >-
  Use this skill on every contribution (before committing or opening a PR) to run
  regular Rust quality and security checks: formatting (cargo fmt), linting
  (cargo clippy), and dependency vulnerability auditing (cargo audit).
---

# Rust Quality & Security Checks

Run these checks on **every contribution** before committing or opening a PR:

## 1. Formatting (`cargo fmt`)

```bash
cargo fmt --check
```

If formatting differs, run `cargo fmt` and re-check.

## 2. Linting (`cargo clippy`)

```bash
cargo clippy --workspace --all-targets
```

- Must compile cleanly and introduce **zero new warnings** in any modified or added code.
- Fix any Clippy warnings in files you touch.

## 3. Dependency Vulnerability Audit (`cargo audit`)

```bash
cargo audit
```

(In offline/sandboxed environments where `advisory-db` cannot be fetched, run `cargo audit --no-fetch`.)

- Must report **0 vulnerabilities**.
- If an advisory affects a direct or transitive dependency, upgrade or replace the affected dependency (respecting `AGENTS.md` pure-Rust crypto rules: never add `ring`, `aws-lc-rs`, or `openssl`).

## 4. Compilation, Unit Tests & End-to-End Validation

A **full quality pass** (before merging PRs or cutting a release) is not complete without **all** of:
1. Formatting: `cargo fmt --all -- --check`
2. Linting: `cargo clippy --workspace --all-targets` (zero new warnings)
3. Dependency audit: `cargo audit` (0 vulnerabilities)
4. Host tests: `RUSTFLAGS="-D warnings" cargo test --workspace` (zero compiler warnings)
5. PS Vita build & ELF audit (when VitaSDK + nightly are available): `just vpk` (zero warnings, `_newlib_heap_size_user` and `sceUserMainThreadStackSize` exported)
6. End-to-End validation: `./.agents/skills/syncthing-e2e-validation/scripts/validate.sh` (covers `quality` + `host` + `vita` + `cli-lan` + `vita3k` outbound & inbound + `internet`; report any `BLOCKED` stage explicitly)

For UI changes, also follow `vitasend-ui-design` and `AGENTS.md` §6 (Vita3K screenshots).
