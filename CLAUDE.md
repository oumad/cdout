# cdout — notes for agents working in this repo

A Windows + macOS desktop assistant in pure Rust: a hotkey opens a spotlight
prompt, the agent sees the file manager's current folder and selection, and
an LLM proposes shell commands for approval. The README covers what it does
and how; this file is what you need to work on it without re-discovering it.

## Layout

- `crates/cdout-core` — UI-free backend (agent loop, LLM clients, tools,
  approval policy, sessions, skills, config, Explorer/Finder bridges). The
  app calls it through `cdout_core::api` and the `ChunkSink` streaming trait.
- `crates/cdout` — the app (egui/eframe): `state/` pure state machine,
  `dispatch.rs` effect runner, `app.rs` windows and message pump, `ui/`
  screens. Its `README.md` lists the env vars, the log-event contract and the
  egui/wgpu gotchas.
- `skills/` bundled skills (compiled in), `icons/`, `scripts/` test and
  packaging scripts, `docs/` the GitHub Pages site (not developer docs).
- The version is set once, in the root `Cargo.toml` under `[workspace.package]`.

## Gates before a commit

```sh
cargo test --workspace                                            # core + app unit, screen and glyph tests
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p cdout --features fake-llm --all-targets -- -D warnings
cargo fmt --all --check
cargo check --workspace --all-targets --target aarch64-apple-darwin  # macOS still compiles (Windows host)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/native-smoke.ps1              # real binary, 7 scenarios
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/explorer-selection-check.ps1  # real Explorer windows
```

With Claude Code or Codex signed in, `cargo run -p cdout-core --example
cli_probe` checks the subscription backends end to end (it uses real quota).

The two scripts need an unlocked Windows desktop; they build their own
binaries, so nothing from `target/` may be running. macOS can only be
compile-checked from Windows; say so when reporting a change that touches
macOS code paths.

## Conventions

- UI logic goes in `crates/cdout/src/state/` (no I/O, injected `Instant`)
  with a unit test; screens are functions of the state and get a headless
  `egui_kittest` test (`crates/cdout/tests/ui_screens.rs`). Anything with a
  window, hotkey or process is covered by the smoke script's log oracle.
- Magic values live in `crates/cdout-core/src/constants.rs` and the constants
  at the top of `crates/cdout/src/state/mod.rs`.
- Platform differences go behind `crates/cdout-core/src/platform.rs` (shell)
  and `crates/cdout/src/platform/` (windows), not inline `cfg!`.
- Log event names (`spotlight.shown`, `agent.step.done kind=…`, …) are
  asserted by `scripts/native-smoke.ps1`; change both together.
- UI strings may only use characters the bundled fonts can draw
  (`tests/glyph_coverage.rs` enforces it); paint icons in `ui/icons.rs`.
- Bundled skills bypass the safety scanner, so adding one is a security
  review; user skills are scanned and quarantined.
- API keys never leave the core in plaintext (`masked_api_keys`).
- Subscription backends (`llm/clients/cli.rs`) only ever run the official,
  unmodified CLI and let it sign in through its own flow. Never read, copy or
  forward a CLI's credential files or tokens, and never delete them.
- Pre-release: no backwards-compatibility constraints on the JSON config shape.
