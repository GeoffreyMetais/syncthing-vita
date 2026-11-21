---
name: syncthing-e2e-validation
description: >-
  Use this skill to validate VitaSyncthing end to end before a PR or a release, or whenever
  a change touches sync behaviour (syncthing-core sessions / TLS / discovery / relays, the
  PS Vita app's sync worker, or syncthing-cli): host tests, the PS Vita build, CLI <-> CLI
  LAN sync, PS Vita app (Vita3K) <-> CLI sync, and the Internet path against the real
  Syncthing servers (Global Discovery, relays).
---

# End-to-End Validation: PS Vita App & `syncthing-cli`

Run the validation script from the repository root:
```bash
./.agents/skills/syncthing-e2e-validation/scripts/validate.sh
```
Select stages with `--only host,cli-lan` or `--skip vita3k`; logs land in
`target/e2e-validation/` (`--out DIR` to change).

| Stage | What it proves | Needs |
| :--- | :--- | :--- |
| `quality` | `cargo fmt --check` passes, `cargo clippy --workspace --all-targets` compiles cleanly, and `cargo audit` reports 0 vulnerabilities | Rust, `cargo-audit` |
| `host` | `cargo test --workspace` passes with zero compiler warnings | Rust |
| `vita` | `just vpk` builds without warnings; `_newlib_heap_size_user` and `sceUserMainThreadStackSize` are exported (C2-12828-1 prevention) | VitaSDK, nightly |
| `cli-lan` | two `syncthing-cli` daemons with `dynamic` addresses find each other by LAN discovery (UDP 21027) and sync a folder both ways | UDP broadcast on the host |
| `vita3k` | outbound Vita -> CLI LAN sync (`scripts/vita3k_cli_sync.sh`, which also supports `internet` and `internet-then-lan` modes) **and** inbound CLI -> Vita TCP 22000 sync + `DeviceOffer` accept/decline E2E (`scripts/vita3k_inbound_e2e.sh`, screenshots in `vita3k/` and `vita3k-inbound/`) | Vita3K, VitaSDK |
| `internet` | relay pool fetched; our announcement accepted by `discovery-announce-v4.syncthing.net` and found back by a lookup; a relay joined; then a CLI <-> CLI sync with no LAN and no address (Global Discovery lookup + relay) | `*.syncthing.net` reachable, outbound TCP to relays (port 22067) |

## Reading the summary

- `PASS` / `FAIL`: the stage ran; any `FAIL` makes the script exit 1. Open the stage's log.
- `BLOCKED`: the environment prevents the stage (missing tool, no broadcast, servers or
  relays unreachable). It is not a regression, but **say so explicitly** in the report: a
  blocked stage was not validated.
- `SKIP`: excluded with `--only` / `--skip`.

## Environment notes

- Behind an HTTP proxy, `syncthing-cli` reaches Global Discovery and the relay pool through
  `HTTPS_PROXY` (`CONNECT`) and trusts `SSL_CERT_FILE` in addition to the public roots (a
  TLS-inspecting proxy re-signs certificates). Connections to peers and relays are always
  direct: where only the proxy reaches the Internet, `internet` ends `BLOCKED` at the relay
  step after validating the pool, the announcement and the lookup.
- An announcement from behind NAT registers the public address of the exit point: a direct
  connection to it only works with a forwarded port, hence the relay step.
- The `vita3k` stage leaves a VPK built with the `ui-script` feature in `target/`: rebuild
  with `just vpk` before publishing anything.
- On failure, the useful logs are: `cli-lan-*.log` and `internet-*.log` (CLI daemons, one
  line per event), `vita3k/vita.log` (the Vita's `syncthing.log`) and `vita3k/cli.log`.
- Diagnostics by hand: `syncthing-cli relays` (relay pool), `syncthing-cli lookup <ID>`
  (Global Discovery), `syncthing-cli discover` (LAN announcements).
