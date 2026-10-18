# cdout-native

cdout as a pure-Rust desktop app (egui/eframe), sharing all backend behaviour
with the Tauri app through `cdout-core`. No webview, no Node.

Status: preview, shipped alongside the Tauri app. Design notes and the
decision log are in `docs/plans/2026-10-08-native-egui-rewrite.md` (§11).

## Run

From `src-tauri/`:

```sh
cargo run -p cdout-native
```

It uses the same data dir as the Tauri app (`%APPDATA%\cdout`,
`~/Library/Application Support/cdout`). To try it without touching your real
config, sessions or keys, and without fighting an installed cdout over the
hotkey:

```powershell
$env:CDOUT_DATA_DIR = "$env:TEMP\cdout-dev"   # isolated data dir (absolute path)
$env:CDOUT_HOTKEY   = "Ctrl+Alt+Shift+F9"     # overrides the configured hotkey
cargo run -p cdout-native
```

With `--features fake-llm` and `CDOUT_FAKE_LLM=text|command|question|long|markdown`
the app talks to a scripted model instead of a provider.

Logs: `<data dir>/logs/cdout-native.log` (`CDOUT_LOG` takes a tracing filter).

GPU backend: on Windows the app uses DirectX 12 (Vulkan on this NVIDIA setup
failed, and took the app down, whenever another process had filled the GPU's
memory; DirectX pages instead). Other platforms use wgpu's default (Metal on
macOS). To override anywhere, set `WGPU_BACKEND` before launching, e.g.
`vulkan`, `dx12`, `metal` or `gl`; the chosen adapter is logged at startup as
`gpu.adapter backend=…`.

## Test

| Layer | Command (from `src-tauri/`) |
|---|---|
| State machine and helpers | `cargo test -p cdout-native --lib` |
| Screens, headless, with pixel snapshots | `cargo test -p cdout-native --tests` |
| Real binary on Windows (hotkey, tray, windows, UI Automation) | `powershell -File ..\scripts\native-smoke.ps1` |
| macOS compile check from Windows | `cargo check -p cdout-native --all-targets --target aarch64-apple-darwin` |

Snapshots are in `tests/snapshots/`; regenerate on purpose with
`UPDATE_SNAPSHOTS=true`. The smoke script needs an unlocked desktop session.

## Package

- Windows: `cargo build -p cdout-native --release` gives a single
  `target/release/cdout-native.exe` with the icon embedded.
- macOS: `scripts/package-macos.sh` builds a universal `cdout.app` and DMG
  into `dist-native/` (ad-hoc signed, like the Tauri build).
