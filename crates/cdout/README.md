# cdout (app crate)

The desktop app: windows, tray, hotkey and screens, built on egui/eframe. All
behaviour that is not presentation lives in `cdout-core`; this crate is UI
only. The repository README has the user-facing picture; this file is for
working on the app.

## Run

From the repo root:

```sh
cargo run
```

It uses the real data dir (`%APPDATA%\cdout`, `~/Library/Application
Support/cdout`). To try it without touching your config, sessions or keys,
and without fighting an installed cdout over the hotkey:

```powershell
$env:CDOUT_DATA_DIR = "$env:TEMP\cdout-dev"   # isolated data dir (absolute path)
$env:CDOUT_HOTKEY   = "Ctrl+Alt+Shift+F9"     # overrides the configured hotkey
cargo run
```

Environment variables the app reads:

| Variable | Effect |
|---|---|
| `CDOUT_DATA_DIR` | Use this directory instead of the platform app-data dir |
| `CDOUT_HOTKEY` | Override the configured chord for this run |
| `CDOUT_LOG` | `tracing` filter (default `warn,cdout=info`) |
| `WGPU_BACKEND` | `vulkan`, `dx12`, `metal` or `gl`. Default: DirectX 12 on Windows, wgpu's choice elsewhere |
| `CDOUT_FAKE_LLM` | With `--features fake-llm`: `text`, `command`, `question`, `long` or `markdown` selects the scripted model's behaviour. `CDOUT_FAKE_LLM_DELAY_MS` slows it down, `CDOUT_TEST_PANIC_MS` crashes the app after the spotlight is shown (the crash scenario) |

Logs go to `<data dir>/logs/cdout.log` and stderr. The log lines named like
`spotlight.shown reason=hotkey` are a contract: `scripts/native-smoke.ps1`
asserts on them, so keep the names when touching the code that emits them
(`app.start`, `gpu.adapter`, `hotkey.*`, `spotlight.*`, `main.*`,
`agent.step.*`, `agent.stream.delta`, `exec.*`, `app.panic`, `crash.restart`,
`toast`).

## Layout

| Module | Role |
|---|---|
| `state/` | Pure state machine. `AppState::update(Msg, Instant) -> Vec<Effect>`; no I/O, time is passed in. `flows.rs` has the spotlight/sessions/settings flows, `agent.rs` the agent loop (approval policy, auto-steps, loop verdicts, queue), `text.rs` the string helpers, `tests.rs` the unit tests |
| `dispatch.rs` | Turns effects into work: tokio for async, a serial thread for file I/O, results back as `Msg`. Stale results are dropped by generation |
| `app.rs` | The eframe `App`. `logic()` pumps messages, registers the spotlight viewport and runs the test crash hook; `ui()` draws the main window |
| `ui/` | One file per screen or widget family. `theme.rs` holds the palette, `icons.rs` paints icons, `widgets.rs` the shared pieces |
| `platform/` | Windows and macOS specifics for windows and activation |
| `hotkey.rs`, `tray.rs`, `fonts.rs`, `crash.rs`, `logging.rs` | As named |

Things that are not obvious from the code:

- The main window is the root viewport; the spotlight is a deferred child
  viewport. eframe attaches the accessibility tree only to the root, so UI
  Automation cannot see the spotlight's widgets (the smoke test proves the
  spotlight's text by submitting it), and `App::ui` is not called while the
  root is hidden, which is why orchestration lives in `App::logic`.
- No transparent windows on Windows: the spotlight window is exactly the
  card, resized to its content every frame. macOS gets a transparent window.
- egui lays out in one pass, so anything centred by measurement
  (`spotlight::centered_row`) is correct from the second frame on; screen
  tests run a few frames before asserting.
- The bundled egui fonts lack many symbols. `tests/glyph_coverage.rs` scans
  the UI sources for characters the fonts cannot draw; add a painted icon in
  `ui/icons.rs` rather than a new glyph.
- Windows pins wgpu to DirectX 12 because Vulkan lost the device, and took
  the app down, whenever another process filled GPU memory. The panic hook
  relaunches the app and the spotlight draft is persisted, so a crash costs
  nothing but a flicker; still, do not switch the default without re-running
  the memory-pressure check.
- `gpu-allocator` must resolve to the same `windows` crate version as
  `wgpu-hal`; if the lockfile drifts and `wgpu-hal` fails to compile, run
  `cargo update -p gpu-allocator`.

## Test

| Layer | Command (repo root) |
|---|---|
| State machine and helpers | `cargo test -p cdout --lib` |
| Screens, headless, with pixel snapshots | `cargo test -p cdout --tests` |
| Real binary on Windows (hotkey, tray, windows, UI Automation, crash + relaunch) | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/native-smoke.ps1` |
| Explorer tab/selection detection against real Explorer windows | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/explorer-selection-check.ps1` |
| macOS compile check from Windows | `cargo check -p cdout --all-targets --target aarch64-apple-darwin` (after `rustup target add aarch64-apple-darwin`) |

Snapshots live in `tests/snapshots/`; regenerate deliberately with
`UPDATE_SNAPSHOTS=true`, and set `CDOUT_SKIP_SNAPSHOTS=1` where the GPU
differs from the recording machine (CI does). The kittest harnesses share a
GPU device and are serialized with a mutex; keep new screen tests on it. The
smoke script needs an unlocked desktop and refuses to type unless cdout owns
the foreground window; a release build must not be running from `target/`
while it builds.

## Package

- Windows: `cargo build --release` gives `target/release/cdout.exe` with the
  icon embedded and no console window (debug builds keep the console so
  panics reach the terminal).
- macOS: `scripts/package-macos.sh` builds a universal `cdout.app`
  (`macos/Info.plist`, bundle id `io.cdout.app`) and a DMG into `dist/`,
  ad-hoc signed.
