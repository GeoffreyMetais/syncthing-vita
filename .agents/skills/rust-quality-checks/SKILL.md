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

## 4. Compilation & Tests

- Host tests: `cargo test --workspace` (zero compiler warnings).
- PS Vita target (when VitaSDK + nightly are available): `just check-vita` or `just vpk` (zero compiler warnings).
- For sync or UI changes, also follow `syncthing-e2e-validation` and `AGENTS.md` §6.
