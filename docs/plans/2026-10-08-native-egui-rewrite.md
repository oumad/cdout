# cdout native rewrite (egui) — execution plan

Status: **planned 2026-10-08, not started.** Written by a planning session (Claude Fable 5.1) for execution by an autonomous agent (Opus 5.5) in a fresh session. Everything the executor needs is in this file plus the reference spike in `docs/plans/native-egui-spike/`.

---

## 0. Read this first

**Goal.** Replace the Tauri + React shell with a pure-Rust desktop app built on egui/eframe, for Windows and macOS, while keeping the existing Rust backend and keeping the Tauri app working until the native app reaches parity. Both shells are built from one shared core crate.

**Owner rules (non-negotiable).**
- Never push to any remote. Work on the local topic branch `feat/native-egui`. Commit locally per phase step; show the diff; stop.
- Commit messages: no `Co-Authored-By` lines.
- Never change repo visibility, collaborators, branch protection.
- Do not rename the local folder `e:\DEV\personal\shuttle-io` (it is the Claude Code project key).
- The Tauri app must keep building and passing its tests after every phase until Phase 4, which only the owner can authorise.
- Pre-release: no backwards-compat constraints on config JSON shape. But both shells share `%APPDATA%\cdout` (`~/Library/Application Support/cdout` on macOS) during the transition, so sessions/config formats are owned by the core and stay identical.

**Validation principle.** Every feature must be verifiable by the executor alone, with no human in the loop. Three tiers:

| Tier | What | Tool |
|---|---|---|
| A | Pure state machine (reducer) and helpers | `cargo test` unit tests |
| B | Screens, headless: accessibility-tree queries, clicks, key presses, pixel snapshots | `egui_kittest` |
| C | The real binary on Windows: tray, global hotkey, window show/hide/focus, streaming end-to-end | `scripts/native-smoke.ps1` (synthesized input + screenshots + log assertions) |

macOS: everything is type-checked from Windows with `cargo check --target aarch64-apple-darwin`. macOS *runtime* behaviour cannot be verified on this machine and must be reported as unverified, never claimed.

---

## 1. Verified facts (2026-10-08, Windows 11, rustc 1.92.0, cargo 1.92.0)

Evidence lives in `docs/plans/native-egui-spike/` (the exact code that produced these results).

**Toolchain and versions**
- `eframe`, `egui`, `egui_extras`, `egui_kittest` **0.35.0** are the newest versions compatible with rustc 1.92 (`cargo info egui` prints `version: 0.35.0 (latest 0.36.2)`; 0.36 needs a newer toolchain). Pin `0.35`.
- `egui_commonmark` **0.24.0** pins `egui = 0.35.0` and `egui_extras = 0.35.0` (features: `better_syntax_highlighting`, `embedded_image`, `macros`).
- `tray-icon` 0.26.1 (re-exports muda as `tray_icon::menu`), `global-hotkey` 0.8.0, `window-vibrancy` 0.8.1, `objc2-app-kit` 0.3.2, `open` 5.4.4, `include_dir` 0.7.4, `tracing` 0.1.44, `tracing-appender` 0.2.5, `tracing-subscriber` 0.3.23, `image` 0.25.10, `cargo-packager` 0.11.8.
- Re-run `cargo info <crate>` before pinning; the toolchain may have moved.

**egui 0.35 API (differs from older tutorials; checked against the local registry source at `%USERPROFILE%\.cargo\registry\src\*\egui-0.35.0\` and `eframe-0.35.0\`)**
- `eframe::App` requires `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`. There is **no `update`**. Optional `fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame)` runs before `ui` each pass; drain event channels there. `fn clear_color(&self, &Visuals) -> [f32; 4]` still exists; return `[0.0; 4]` for a transparent window.
- Panels take `&mut Ui`, not `&Context`: `egui::CentralPanel::default().show(ui, |ui| ..)`, `egui::Panel::left("id").show(ui, ..)` (also `right`/`top`/`bottom`). `SidePanel`/`TopBottomPanel` are gone.
- `egui::Frame::NONE`, `egui::Frame::new()`, `.corner_radius(CornerRadius::same(u8))`, `.inner_margin(Margin::same(i8))`, `.fill(Color32)`, `.stroke(Stroke::new(w, color))`.
- `ViewportBuilder`: `with_inner_size`, `with_position`, `with_decorations(false)`, `with_transparent(true)`, `with_always_on_top()`, `with_taskbar(false)`, `with_resizable(false)`, `with_title`, `with_icon`.
- `ViewportCommand`: `Visible(bool)`, `Focus`, `OuterPosition(Pos2)`, `InnerSize(Vec2)`, `StartDrag`, `Minimized(bool)`, `Maximized(bool)`, `Close`, `WindowLevel`, `Decorations(bool)`, `Icon`, `RequestUserAttention`. Send with `ctx.send_viewport_cmd(..)` or `ctx.send_viewport_cmd_to(viewport_id, ..)`.
- `ctx.input(|i| i.viewport().focused)` is `Option<bool>`; `ViewportInfo` also has `monitor_size`, `inner_rect`, `outer_rect`, `minimized`, `maximized`, `close_requested`, `events`.
- `ctx.show_viewport_immediate(id, builder, |ctx, class| ..)` / `show_viewport_deferred(..)`: must be called **every pass** the child should exist. Deferred needs `Arc<Mutex<State>>`; immediate is `FnOnce`, simpler, but parent and child repaint together.

**Spike results on Windows (`renderer: eframe::Renderer::Wgpu`)**

> **Correction (executor, 2026-10-08):** the transparency claim below was wrong. The dark area around the spike's card was an opaque black clear, not the desktop. A precise check (magenta backdrop behind the window) shows eframe 0.35 cannot make a window transparent on Windows; see §11.
- A root viewport with `decorations(false) + transparent(true) + always_on_top + taskbar(false)` renders a rounded dark card with the desktop visible around the corners (screenshot taken, transparency confirmed).
- Tray icon created inside the `app_creator` closure; `tray_icon::menu` menu works; `TrayIconEvent::set_event_handler`, `MenuEvent::set_event_handler`, `GlobalHotKeyEvent::set_event_handler` closures forward to an `mpsc` channel and call `ctx.request_repaint()`.
- `"ctrl+alt+shift+F9".parse::<HotKey>()` registered; a chord synthesized with `keybd_event` produced `Pressed`/`Released` events; toggling `ViewportCommand::Visible` hid and re-showed the window; `Visible(true)` + `Focus` yielded `focused = Some(true)`.
- **While the root viewport was hidden, passes still ran**: the hotkey handler's `request_repaint()` reached `App::ui` and the window came back. A hidden root can therefore keep reacting to events.
- `"ctrl+alt+a"` failed with `AlreadyRegistered` because the installed Tauri cdout (`cdout.exe`) was running and owns `Ctrl+Alt+A`. Development builds must use a different chord and data dir (§4.3).
- `egui_kittest`: `Harness::new_ui`, `get_by_label("..").click()`, `run()`, `snapshot("name")` work headless on this machine. First run with `UPDATE_SNAPSHOTS=true` writes `tests/snapshots/<name>.png`; later runs compare and pass. 0.6 s per test.
- `cargo check --target aarch64-apple-darwin` of the spike (eframe + wgpu + tray-icon + global-hotkey) succeeds on Windows after `rustup target add aarch64-apple-darwin` (already installed on this machine). All `#[cfg(target_os = "macos")]` code can be type-checked here.

**Backend coupling to Tauri (what Phase 0 must cut)**
- `src-tauri/src/features/agent.rs`: `tauri::ipc::Channel<StreamChunk>` in `call_stream_with_context_recovery` and `run_agent_step_stream` (lines ~304 and ~355).
- `src-tauri/src/utils/config.rs`: test `default_hotkey_parses` uses `tauri_plugin_global_shortcut::Shortcut` (line ~449).
- `src-tauri/src/lib.rs`: `app_handle.path().resource_dir()` for bundled skills; everything else in lib.rs is command glue or window/tray/hotkey glue.
- Every other file under `src-tauri/src/` (constants, platform, features/*, llm/*, tools/*, utils/*) is framework-free.

**Baselines on `master` @ `da67c5f`**: `cargo test --lib` → 161 passed (Windows); `npm test` → 70 passed (10 files).

---

## 2. Target architecture

### 2.1 Workspace layout

```
src-tauri/
  Cargo.toml                 # workspace root AND the Tauri shell package (name `cdout`, lib `cdout_lib`)
  Cargo.lock                 # shared
  build.rs, tauri.conf.json, Info.plist, entitlements.plist, icons/, skills/   # unchanged
  src/lib.rs, src/main.rs    # Tauri commands become thin wrappers over cdout-core
  crates/
    cdout-core/              # all framework-free backend code, MOVED (git mv), not rewritten
    cdout-native/            # the egui app (new)
```

`src-tauri` stays the workspace root on purpose: `ci.yml`, `release.yml` and `rust-cache` (`workspaces: "./src-tauri -> target"`) keep working, and the executor cannot run CI to validate path changes. Renaming to `crates/` at the repo root is Phase 4.

Dependency direction: `cdout` (Tauri) → `cdout-core` ← `cdout-native`. The core never depends on egui or tauri.

### 2.2 Native app process model
- One process. winit event loop on the main thread (eframe). A tokio multi-thread runtime is created in `main` before `eframe::run_native`; its `Handle` is stored in the app.
- All I/O (LLM streaming, shell execution, Ollama probes, session writes, Explorer/Finder queries) runs on tokio tasks or `spawn_blocking`. Results come back through `std::sync::mpsc::Sender<UiEvent>`; every send is followed by `ctx.request_repaint()`. The UI thread drains the receiver at the top of `App::logic`.
- Tray, hotkey and menu events are forwarded the same way from their `set_event_handler` closures (exactly as in the spike).
- UI code never touches the network or filesystem directly. It emits `Action`s; a dispatcher turns them into tasks. This is what makes the screens testable with kittest.

### 2.3 Window model

> **Superseded (executor, 2026-10-08):** the roles are reversed in the implementation. The main window is the root viewport and the spotlight is the deferred child, and orchestration runs in `App::logic`. Reasons in §11.
- **Root viewport = spotlight.** It always exists; it is hidden/shown with `ViewportCommand::Visible`. Transparent, undecorated, always on top, no taskbar entry, visible on all Spaces (macOS), no shadow.
- **Main window = child viewport**, shown with `ctx.show_viewport_immediate` whenever `state.main_open` is true, with `decorations(false)` + `transparent(true)` and a custom title bar (as Tauri does today). Closing it sets `main_open = false` (close-to-tray). Start with immediate viewports; switch to deferred (`Arc<Mutex<MainState>>`) only if the double repaint measurably hurts.
- **Must-verify first in Phase 1 (step 1):** a child viewport stays interactive while the root is hidden. The spike proved hidden-root passes run; it did not open a child. If this fails, **Plan B** is one window that morphs between spotlight and main layouts (resize + reposition + decorations toggle). Decide on evidence and record the outcome in §11.

### 2.4 Startup behaviour (parity)
Tauri shows the main window on launch, spotlight hidden, tray always present, main close → hide to tray, quit only from tray. Native does the same.

### 2.5 UI state machine (ported from `src/App.tsx` on master)
Implement `cdout-native/src/state/` with **no egui types**: `AppState`, `SpotlightState`, `MainState`, `enum Action` (user intents), `enum UiEvent` (async results), `fn reduce(&mut AppState, UiEvent|Action) -> Vec<Effect>`. Effects (`StartStep`, `ExecuteCommand`, `CreateSession`, `SaveSession`, `RegisterHotkey`, `ShowMain`, `HideSpotlight`, …) are executed by the dispatcher. Unit-test the reducer branch by branch. The React logic to port, with App.tsx line ranges (read them before writing code):

- **Spotlight** (App.tsx 68–135, `src/constants.ts`): on focus gained record `shown_at`, refresh file-manager context, focus the input; on focus lost hide only if `now - shown_at > BLUR_GRACE_MS (300 ms)`; Escape hides; submit requires non-empty prompt; if no model: toast "No model set up yet. Open cdout (from the tray) and pick a provider — local Ollama or a cloud key — to get started."; submit with the modifier held (Ctrl on Windows, Cmd on macOS) means `auto_approve = true`; a successful submit clears the prompt, hides spotlight, centers main on the cursor's monitor, shows main, dispatches `SpotlightSubmitted { prompt, model, auto_approve }`.
- **SpotlightSubmitted** (476–523): flush pending session save; hard-reset UI (cancel stream, clear pending command/question, bump run generation, clear streaming text); set model; fetch file-manager state; create a new session (core seeds system + user messages); set history; `is_processing = true`; if `auto_approve` set `auto_execute`; run step with `should_auto_execute = auto_approve || auto_execute`.
- **startAgent / queue** (563–660): busy = processing || executing || streaming text non-empty || pending command || pending question || auto-step pending → queue the prompt (chips with remove); drain one queued prompt when idle. Reset `auto_step_count`, `consecutive_failures`, pending command/question; call core loop-detector reset. Empty history → new session; else if `sync_pending` push a user message `[CONTEXT UPDATE - Files have changed]` in one of two formats (inline numbered list, or when > `FILE_LIST_THRESHOLD (20)` files write the list with core `write_file_list` and include the platform `read_list_hint`), then push the user prompt; run step.
- **runAgentStep** (690–900): capture generation; consume `drop_tools_next`; stream (`TextDelta` appends to streaming text unless stale/stopped); on completion discard if stale; clear streaming text; if stop requested, return; history = `updated_history`. Normalise proposals: `ToolProposals` as is; legacy `CommandProposal` → one proposal with `tool_name = platform.shell_tool_name`. Select proposal: `ask_user_question` wins, else first.
  - Branch 1 question → `pending_question { data, tool_call_id }`, `consecutive_failures = 0`, `auto_execute = false`.
  - Branch 2 command → remember `tool_call_id`; `auto_step_count += 1`; loop verdict `Block`/`Break` → `auto_execute = false`, `is_processing = false`, `Break` sets `drop_tools_next`, append synthetic user message `[Loop detected] {reason} — stop, reassess, and either ask a clarifying question with ask_user_question or wait for the user's guidance.`, `pending_command = cmd`, return. Else classify risk via core (`mutating` if the classifier errors); `permitted = should_auto || mode == Auto || (mode == ReadOnly && risk == ReadOnly)`; `may_run = permitted && risk != Dangerous`; if `may_run && auto_step_count < MAX_AUTO_STEPS (10)` → execute with cwd = file-manager path; failure = output matches `\[Exit code:\s*-?\d+\s*—\s*Failed\]`; `consecutive_failures >= MAX_CONSECUTIVE_FAILURES` (value in `src/constants.ts`) → reset counter, `auto_execute = false`, append synthetic `[Auto-execute paused] {N} commands failed in a row. Stop, re-read the errors above, and either explain what's going wrong or wait for the user's guidance instead of trying more variants.`, return; else push tool message and recurse with the *same* `should_auto`. Execution error → `auto_execute = false`, `pending_command = cmd`. Not permitted → `pending_risk = risk`, `pending_command = cmd`.
  - Branch 3 text → if `should_auto` and `!is_task_complete(text)` and `auto_step_count < 10` → after `AUTO_STEP_DELAY_MS (500)` append the synthetic nudge (exact text at App.tsx 877–881: "If the task is complete, reply with exactly 'Task Complete' and stop. …") and run step with `should_auto = true`; else `auto_execute = false`.
- **answerQuestion / dismissQuestion** (908–947): push a `tool` message whose `tool_calls` echoes the question's `tool_call_id` with function name `ask_user_question`; dismissal content `[User dismissed the question without answering.]`.
- **stopAgent** (948–985): set stop flag, `auto_execute = false`, core `cancel::request()`, keep partial assistant text as an assistant message, append synthetic `[Interrupted by user] The user stopped this action. STOP what you are doing and wait for the user to tell you how to proceed.`, clear pending state, reset loop detector; if a command is running show the kill dialog (pid, preview) → core `kill_running`.
- **Approve / reject / edit / "All"** (App.tsx 990–1150; read for the exact tool-result strings on reject and dismiss), approval mode persistence (`get/set_approval_mode`).
- **Sessions** (`src/hooks/useSessions.ts`): autosave 500 ms after any history change; flush on switch, new chat, spotlight submit, window close; list sorted by `last_active_at` desc; rename/delete.
- **Models** (`src/hooks/useModels.ts`): provider grouping, 5 s poll only while no model is usable, stale-slug reconciliation with a toast, persisted selection.
- **is_task_complete**: port `src/utils/agent.ts` (keywords in `src/constants.ts`: `COMPLETION_KEYWORDS`, `COMPLETION_SHORT_KEYWORDS`, `COMPLETION_SHORT_MAX_LEN = 200`) with the same cases as its vitest file.

---

## 3. Phase 0 — extract `cdout-core` (no behaviour change)

1. The planning session already created the local branch `feat/native-egui` from master and committed this plan on it. Confirm with `git branch --show-current`; if you are elsewhere, `git switch feat/native-egui`.
2. `mkdir -p src-tauri/crates/cdout-core/src` and `git mv` these into it: `src/constants.rs`, `src/platform.rs`, `src/features/`, `src/llm/`, `src/tools/`, `src/utils/`. New `crates/cdout-core/src/lib.rs`:
   ```rust
   pub mod constants; pub mod platform; pub mod features; pub mod llm; pub mod tools; pub mod utils; pub mod api;
   ```
   `crates/cdout-core/Cargo.toml` takes every dependency from `src-tauri/Cargo.toml` except `tauri*`, plus `include_dir = "0.7"`; keep the `[target.'cfg(windows)']` and `[target.'cfg(unix)']` tables exactly.
3. **Streaming sink** in `features/agent.rs`:
   ```rust
   pub trait ChunkSink: Send + Sync { fn send(&self, chunk: StreamChunk); }
   impl<F: Fn(StreamChunk) + Send + Sync> ChunkSink for F { fn send(&self, c: StreamChunk) { self(c) } }
   pub async fn run_agent_step_stream(model: String, history: Vec<Message>, drop_tools: bool, sink: std::sync::Arc<dyn ChunkSink>) -> Result<AgentStepResult, String>
   ```
   `call_stream_with_context_recovery` takes `&dyn ChunkSink`. Tauri wrapper: `let ch = channel.clone(); Arc::new(move |c: StreamChunk| { let _ = ch.send(c); })`.
4. Move the `default_hotkey_parses` test into the Tauri crate (`lib.rs` test module) unchanged. Phase 1 adds the equivalent test in `cdout-native` with `global_hotkey::hotkey::HotKey` (the same type the Tauri plugin re-exports).
5. **Skills**: in core add `pub static BUNDLED_SKILLS: include_dir::Dir<'static> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../skills");` and `pub fn load_all_skills_embedded() -> LoadedSkills` (bundled from the embedded dir, user skills from the data dir, same scanner/quarantine path). Keep `load_all_skills(resource_dir)` for Tauri; do not change Tauri's behaviour.
6. **Facade** `crates/cdout-core/src/api.rs`: move the logic currently inside Tauri commands into plain functions with identical semantics and move their serialisable types with them: `provider_status().await`, `platform_info(list_file_path)`, `init_agent_conversation(context_path, selected_files, user_prompt, model, &LoadedSkills)`, `write_file_list(files)`, `has_legacy_credentials()`, `cleanup_legacy_credentials()`, `ollama_models().await`, `open_file_access_settings()`, `list_skills(&LoadedSkills) -> ListSkillsResult`. `lib.rs` commands become one-line wrappers. Command names and JSON shapes do not change, so the frontend is untouched.
7. **Data-dir override** for isolated runs and tests: a single `data_dir()` resolver in core honours `CDOUT_DATA_DIR` (absolute path) before the platform default. Make sessions, config, API keys, user skills and logs all go through it (verify with `rg -n "config_dir|data_local_dir|data_dir" crates/cdout-core/src`). `migrate_legacy_data_dir` is skipped when the override is set.
8. **Fake LLM for deterministic end-to-end tests**: cargo feature `fake-llm` on core. When enabled *and* `CDOUT_FAKE_LLM` is set, `llm::router::route_chat_stream` ignores the network and streams a scripted reply chosen by the env value: `text` → "Hello from the fake model." as 5 deltas then `Done`; `command` → a proposal of the platform shell tool with command `echo cdout-fake` ; `question` → an `ask_user_question` proposal with two options. Also make `ollama_models()` return `["ollama:fake"]` under the same condition so the UI has a usable model. Never enabled in release builds (not a default feature).
9. Workspace: in `src-tauri/Cargo.toml` add
   ```toml
   [workspace]
   members = ["crates/*"]
   resolver = "2"
   ```
   and `cdout-core = { path = "crates/cdout-core" }` under `[dependencies]`. Update `use crate::…` in lib.rs to `use cdout_core::…`.
10. CI (edit only; the executor cannot run it; every CI command must also be run locally): in `.github/workflows/ci.yml` change `cargo test --lib` → `cargo test --workspace --lib`, `cargo clippy --lib --all-targets -- -D warnings` → `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` → `cargo fmt --all --check`. Keep `working-directory: ./src-tauri`.

**Exit criteria (all run from `src-tauri/`, all must pass):**
```
cargo test --workspace --lib            # >= 161 passing, 0 failed; `cargo test --workspace --lib -- --list | sort` has the same names as before the move (diff it)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build -p cdout                    # Tauri shell compiles (needs `npm ci` done once at repo root)
cargo check -p cdout-core --target aarch64-apple-darwin
cargo check -p cdout-core --features fake-llm --target aarch64-apple-darwin
cd .. && npm test                       # 70 passing
npm run tauri build -- --debug          # produces src-tauri/target/debug/cdout.exe; launch it with CDOUT_DATA_DIR=<temp> for 5 s and confirm the process stays alive (tasklist), then kill it
```
Commit: `refactor: extract cdout-core from the Tauri crate`.

---

## 4. Phase 1 — native spotlight + minimal main window

This milestone exists to burn down every platform risk before any screen work.

### 4.1 Crate `src-tauri/crates/cdout-native`
```toml
[package]
name = "cdout-native"
version = "0.1.0"
edition = "2021"

[dependencies]
cdout-core = { path = "../cdout-core" }
eframe = { version = "0.35", default-features = false, features = ["default_fonts", "wgpu", "accesskit"] }
egui_extras = "0.35"
egui_commonmark = { version = "0.24", features = ["better_syntax_highlighting"] }
tray-icon = "0.26"
global-hotkey = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
open = "5"
image = { version = "0.25", default-features = false, features = ["png"] }   # decode icons/icon.png for tray + window icon

[target.'cfg(windows)'.dependencies]
windows = { version = "0.52", features = ["Win32_Foundation", "Win32_Graphics_Gdi", "Win32_UI_WindowsAndMessaging"] }

[target.'cfg(target_os = "macos")'.dependencies]
objc2-app-kit = { version = "0.3", features = ["NSScreen", "NSEvent", "NSWindow", "NSApplication", "NSResponder", "NSRunningApplication"] }
objc2-foundation = "0.3"
objc2 = "0.6"            # use whatever objc2 version objc2-app-kit 0.3.2 depends on (check `cargo tree -p objc2-app-kit`)

[dev-dependencies]
egui_kittest = { version = "0.35", features = ["snapshot", "wgpu"] }

[build-dependencies]
winresource = "0.1"      # Windows-only use inside build.rs (icon.ico + version info)
```
`src/main.rs` carries `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.

### 4.2 Modules
```
src/main.rs            tokio runtime, tracing init, NativeOptions (root = spotlight), run_native
src/app.rs             eframe::App impl: logic() drains UiEvent/Action, ui() draws spotlight, shows main viewport
src/dispatch.rs        executes Effects: spawns tasks, sends UiEvents, registers hotkey, window commands
src/state/{mod,spotlight,main,sessions,models,agent}.rs   pure state + reducer (no egui)
src/ui/{theme,spotlight,title_bar,chat,chat_input,approval,question,sidebar,settings,onboarding,toast}.rs
src/hotkey.rs          HotKey parse/normalise, (re)register with unregister_all first, surface AlreadyRegistered
src/tray.rs            tray + menu (Show cdout / Quit cdout), template icon on macOS
src/platform/{mod,windows,macos}.rs   cursor_monitor_rect(), after_window_created(handle) [macOS: all-Spaces, no shadow], dock Reopen
src/skills.rs          wrapper over cdout_core::features::skills::load_all_skills_embedded
src/logging.rs         tracing to <data_dir>/logs/cdout-native.log (daily rolling) + stderr
```

### 4.3 Development isolation (mandatory)
- `CDOUT_DATA_DIR` (from Phase 0) → the smoke script and all manual runs point at a temp dir so the installed Tauri cdout's config/sessions are never touched.
- `CDOUT_HOTKEY` env var overrides the configured hotkey (logged). The smoke script uses `Ctrl+Alt+Shift+F9` because `Ctrl+Alt+A` is held by the installed `cdout.exe` on this machine.
- `CDOUT_FAKE_LLM` + `--features cdout-core/fake-llm` for end-to-end runs without network.

### 4.4 Features in this phase
1. Tray with "Show cdout" / "Quit cdout" (quit exits the process; show opens main). macOS: `with_icon_as_template(true)`.
2. Global hotkey from config (default `Ctrl+Alt+A` / `Cmd+Alt+A`), `CDOUT_HOTKEY` override, `AlreadyRegistered` surfaced as a persistent toast with the chord name.
3. Spotlight root viewport 600×260, transparent, undecorated, always on top, no taskbar; macOS: `NSWindow.collectionBehavior |= CanJoinAllSpaces | FullScreenAuxiliary`, `hasShadow = false`. Show on hotkey: compute cursor monitor rect (Windows: `GetCursorPos` + `MonitorFromPoint` + `GetMonitorInfoW`; macOS: `NSEvent.mouseLocation` + `NSScreen.screens`, flip the y axis), `OuterPosition(center)`, `Visible(true)`, `Focus`. Hide on Escape, on blur after the 300 ms grace, after submit.
4. Spotlight contents: prompt input (auto-focused when shown), model selector (core model list; provider groups), file-manager context line (path + "N files"), refresh button, error toast; Enter submits, Ctrl/Cmd+Enter submits with `auto_approve`.
5. Minimal main viewport 1100×760: custom title bar (drag, minimise, maximise, close-to-tray), a transcript that shows user/assistant/tool messages as plain text, the live streaming text with a caret, and the first proposal rendered as read-only text with a "Run" / "Reject" pair wired to the reducer. No sidebar/settings yet.
6. Submit → create session → stream first step → render, using the real core (`fake-llm` only in tests).
7. Logging with stable event names (the smoke script greps these exact strings): `app.start`, `tray.created`, `hotkey.registered chord=…`, `hotkey.error …`, `hotkey.pressed`, `spotlight.shown`, `spotlight.hidden reason=(escape|blur|submit|hotkey)`, `spotlight.submitted`, `main.shown`, `main.hidden`, `agent.step.start`, `agent.stream.delta`, `agent.step.done kind=(text|command|question)`, `agent.step.error …`, `exec.start`, `exec.done`.

### 4.5 Tests
- **Tier A**: reducer tests for every spotlight rule in §2.5; `cursor_monitor_rect` math as a pure function `center_in(monitor: Rect, window: Vec2) -> Pos2` with multi-monitor and negative-origin cases; hotkey parse/normalise (`Ctrl+Alt+A`, `Cmd+Alt+A`, lower-case input, modifier order).
- **Tier B** (kittest, `Harness::new_ui_state(|ui, state| ui::spotlight::show(ui, state), SpotlightState::default())`): typing then Enter emits `Action::Submit { auto_approve: false }`; `key_press_modifiers(Modifiers::CTRL, Key::Enter)` emits `auto_approve: true`; Escape emits `Hide`; empty prompt emits nothing; path and "3 files" label present; no-model error text present. Snapshots `spotlight_idle`, `spotlight_with_context`, `main_minimal_streaming` at fixed size and `pixels_per_point(1.0)`.
- **Tier C** `scripts/native-smoke.ps1` (start from `docs/plans/native-egui-spike/drive.ps1`): builds `cargo build -p cdout-native --features cdout-core/fake-llm`; sets `CDOUT_DATA_DIR=<temp>`, `CDOUT_HOTKEY=Ctrl+Alt+Shift+F9`, `CDOUT_FAKE_LLM=text`; launches; polls the log (≤ 10 s) for `tray.created` and `hotkey.registered`; sends the chord → expects `spotlight.shown` within 1 s, screenshot `smoke_1_spotlight.png`; sends `VK_ESCAPE` → `spotlight.hidden reason=escape`; chord again → shown; types `hello` + Enter via `keybd_event` → expects `spotlight.submitted`, `main.shown`, `agent.step.start`, ≥ 1 `agent.stream.delta`, `agent.step.done kind=text`; screenshot `smoke_2_main.png`; stops the process; exits non-zero on any missing expectation and prints the log tail. Second invocation with `CDOUT_FAKE_LLM=command` must reach `agent.step.done kind=command` and show the proposal.
- **Cross-check**: `cargo check -p cdout-native --all-targets --target aarch64-apple-darwin`.

### 4.6 Exit criteria
All Phase 0 checks still green, plus:
```
cargo test -p cdout-native                                   # tiers A+B green, snapshots committed
cargo clippy -p cdout-native --all-targets -- -D warnings
cargo check -p cdout-native --all-targets --target aarch64-apple-darwin
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/native-smoke.ps1   # exit 0, screenshots produced
```
Executor opens both screenshots (Read tool) and confirms: rounded card with transparent corners, prompt text visible, main window with streamed text. Record the viewport decision (§2.3) in §11. Commit: `feat(native): egui spotlight, tray, hotkey, minimal main window`.

---

## 5. Phase 2 — main window parity

Port component by component. Each row must end with tests before moving on. Sizes are the current TSX line counts, to budget effort.

| React source | Native module | Behaviour to match | Tests |
|---|---|---|---|
| `TitleBar.tsx` (91) + `usePlatform.ts` | `ui/title_bar.rs` | drag (`StartDrag` on `drag_started()` of the empty area), double-click maximise, min/max/close buttons, close = hide to tray, macOS control placement as the component does | kittest clicks emit actions; snapshot |
| `SessionsSidebar.tsx` (187) + `useSessions.ts` (152) | `ui/sidebar.rs`, `state/sessions.rs` | collapsible, list by `last_active_at` desc, New chat, rename inline, delete with confirm, switch flushes save, 500 ms debounced autosave | reducer tests for debounce/flush; kittest list/rename/delete |
| `ChatMessage.tsx` (190) | `ui/chat.rs` | roles user/assistant/tool/system; `synthetic` → compact gray "cdout internal" note; tool output collapsed > 200 chars; messages collapsed > 500 chars with expand; markdown via `CommonMarkViewer` + one `CommonMarkCache`; streaming text raw with caret; auto-scroll when near bottom | kittest: collapse/expand, synthetic rendering, markdown table renders; snapshot |
| `CommandApproval.tsx` (167) | `ui/approval.rs` | risk badge (read_only / mutating / dangerous; amber/red), editable multiline command, Run / Run all / Reject, dangerous never unattended, keyboard shortcuts as in the component | kittest for each button → action; snapshot per risk |
| `QuestionApproval.tsx` (159) | `ui/question.rs` | options, free-text answer, dismiss | kittest |
| `ChatInput.tsx` (124) | `ui/chat_input.rs` | multiline, Enter submit / Shift+Enter newline, Stop while busy, queued prompt chips with remove | kittest |
| `ModelSelector.tsx` (115) + `useModels.ts` (201) | `ui/model_select.rs`, `state/models.rs` | provider groups (Local / Anthropic direct / OpenRouter), free-model toggle, stale-slug reconciliation toast, 5 s poll only while empty | reducer tests; kittest |
| `SettingsPage.tsx` (578) + `HotkeyRecorder.tsx` (241) | `ui/settings.rs`, `ui/hotkey_recorder.rs` | sections: General (hotkey recorder: capture chord from `ui.input(|i| i.events)`, normalise with `MOD_ORDER` Ctrl, Alt, Shift, Cmd/Super, apply = re-register then persist, show OS glyphs), approval mode; Providers (OpenRouter key masked + preview, Anthropic key, Ollama URL, show free models, disclosure ack); Skills (list + quarantined with reasons); File access diagnostics; Legacy credential cleanup | kittest per section; snapshot per section |
| `ProviderSetup.tsx` (289) | `ui/onboarding.rs` | shown when Ollama reachability is known and no model is usable; Local vs Cloud paths; auto-dismiss when a model appears | kittest; snapshot |
| `MigrationBanner.tsx` (107) | `ui/banner.rs` | one-shot banner + cleanup button | kittest |
| `FileAccessDiagnostics.tsx` (167) | `ui/diagnostics.rs` | Windows: explorer debug payload; macOS: automation authorised flag + "Open System Settings" (core `open_file_access_settings`) | kittest with both payload shapes |
| `ErrorToast.tsx` + `useError.ts` | `ui/toast.rs` | 8 s auto-dismiss, manual close | reducer timer test |
| Kill dialog (App.tsx ~1270) | `ui/kill_dialog.rs` | pid + command preview, Kill / Keep | kittest |
| Context panel (App.tsx ~1460) | `ui/context_panel.rs` | collapsible top panel: path, selected files, refresh, "sync pending" indicator | kittest |
| `ErrorBoundary.tsx` | `logging.rs` | `std::panic::set_hook` writes the panic to the log before exit | unit test of the hook formatter |
| macOS dock Reopen (lib.rs 876–886) | `platform/macos.rs` | observe `NSApplicationDidBecomeActiveNotification`; if no window visible → show main | cross-check only; report unverified |

Also in this phase: full reducer parity with §2.5 (every branch has a test), fonts (bundle a monochrome emoji fallback such as Noto Emoji through `FontDefinitions` so emoji from models are not boxes; note CJK/Arabic as a known limitation), theme tokens matching the Tailwind palette (gray-950 background, indigo user accents, emerald assistant/tool, amber warnings).

**Exit criteria**: every table row ticked with its tests; `scripts/native-smoke.ps1` extended with `CDOUT_FAKE_LLM=command` → approve via the keyboard shortcut → `exec.done` → second step; and `CDOUT_FAKE_LLM=question` → answer → next step; snapshots for every screen at 1100×760 and the spotlight at 600×260; all Phase 0/1 checks green. Commit per component.

---

## 6. Phase 3 — packaging and CI

- **Windows**: `build.rs` embeds `../../icons/icon.ico` and version info with `winresource` (guard with `if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")`). Release artifact `cdout-native-<version>-windows-x64.zip` (exe + README) uploaded alongside the Tauri artifacts until Phase 4.
- **macOS**: `cargo-packager` config under `[package.metadata.packager]` producing `.app` and `.dmg`; bundle id **`io.cdout.app`** (TCC Automation grants are per bundle id); copy the keys from `src-tauri/Info.plist` (`NSAppleEventsUsageDescription`, `LSApplicationCategoryType`); entitlements only `com.apple.security.automation.apple-events` (drop `allow-jit`, which existed for WebKit); universal binary: build `aarch64-apple-darwin` and `x86_64-apple-darwin`, `lipo -create`; unsigned like today (README keeps the right-click → Open note). Executor writes this; it can only be verified by CI on `macos-latest`, so mark it unverified in the report.
- **CI**: `ci.yml` adds `cargo build -p cdout-native` on both runners and `cargo test -p cdout-native` with `CDOUT_SKIP_SNAPSHOTS=1` (snapshot tests early-return when set, until a wgpu adapter on the runners is confirmed). `release.yml` gets a native job per OS. Executor writes the YAML; the owner triggers CI after approving a push.

---

## 7. Phase 4 — switchover (owner decision; do not execute autonomously)
Delete the Tauri shell and the React frontend, move `src-tauri/crates/*` to `crates/*`, drop `macOSPrivateApi` and `allow-jit`, update README, site and workflows, rename binaries to `cdout`. Only after the owner has used the native build and said so.

---

## 8. Validation toolkit (how the executor proves things alone)

| Check | Command | Expectation |
|---|---|---|
| Unit + headless UI | `cd src-tauri && cargo test --workspace` | all green; counts reported |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check` | clean |
| macOS compile | `cargo check -p cdout-core -p cdout-native --all-targets --target aarch64-apple-darwin` | `Finished` |
| Frontend unchanged | `cd .. && npm test && npx tsc --noEmit` | 70 passing |
| Tauri shell | `cargo build -p cdout` (and once per phase `npm run tauri build -- --debug`) | builds |
| Real binary (Windows) | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/native-smoke.ps1` | exit 0, screenshots, log excerpts |

Rules of the toolkit:
- Snapshot tests: fixed harness size, `pixels_per_point(1.0)`, dark visuals, `default_fonts` only. Run with `UPDATE_SNAPSHOTS=true` only when a visual change is intended, and say so in the commit. Commit PNGs under `crates/cdout-native/tests/snapshots/`. Snapshot tests return early when `CDOUT_SKIP_SNAPSHOTS=1`.
- Screens are plain functions `fn show(ui: &mut egui::Ui, state: &mut XState, out: &mut Vec<Action>)` so kittest can drive them with `Harness::new_ui_state`. Query by label/role (`get_by_label`, `get_by_role_and_label`, `query_by_label`), drive with `.click()`, `key_press`, `key_press_modifiers`, `event(Event::Text(..))`, then `run()`.
- Logs are the oracle for Tier C. Keep the event names in §4.4 stable; the smoke script greps them.
- PowerShell is DPI-unaware: screen coordinates it sees are logical, which matches egui's logical window position. Capture a generous region around the window.
- `windows_subsystem = "windows"` is release-only; debug builds keep a console so panics reach the smoke script's stderr capture.
- After any change under `platform/` or behind `cfg(target_os)`, re-run the darwin cross-check before committing.
- What this machine cannot verify, and must be listed as unverified in every report: macOS runtime (Metal transparency, Spaces behaviour, template tray icon, dock Reopen, TCC prompt, hotkey on Carbon), IME input, screen readers, packaging on macOS.

---

## 9. Known losses to accept or mitigate
- Text input is egui's: no spellcheck, dictation or system emoji picker; IME through winit. Accept.
- Typography: no subpixel antialiasing, no complex shaping (Arabic/Hebrew), emoji monochrome at best. Mitigate with a bundled monochrome emoji fallback font; document the rest.
- Markdown: `egui_commonmark` covers CommonMark, tables and highlighted code. No HTML. Accept.
- Transitions/blur: use `window-vibrancy` later if wanted; skip animations.
- Drop shadow on the undecorated main window: none by default. Acceptable for now; `DwmExtendFrameIntoClientArea` is a later polish item on Windows.

---

## 10. Reporting format for the executor
After each phase, the final message must contain: the exact commands run with their summary lines (test counts before/after, clippy result, cross-check result, smoke script exit code); paths of screenshots produced; the list of unverified items; `git diff --stat` of the phase; and the decision log entries added to §11. Never claim macOS runtime behaviour works. Never push.

---

## 11. Execution log (append-only, filled by the executor)
- 2026-10-08 — plan written; spike facts in §1; no implementation started.
- 2026-10-08 — **Phase 0 done** (commit `f37ea66`). `cdout-core` extracted with `git mv`; Tauri commands are thin wrappers over `cdout_core::api`. Test names before/after diffed: all 161 kept (the default-hotkey test moved to the Tauri crate). The Tauri debug build runs against `CDOUT_DATA_DIR` and writes only there. Note for anyone launching a second Tauri cdout while the installed one runs: set `WEBVIEW2_USER_DATA_FOLDER` to a temp dir, or the second webview stays white (shared WebView2 profile).
- 2026-10-08 — **Phases 1 and 2 implemented together** in `crates/cdout-native` (the state machine was cheaper to port whole than in two passes). Decisions and findings:
  - *Window roles reversed (supersedes §2.3).* eframe 0.35 attaches AccessKit only to the root viewport (`eframe/src/native/wgpu_integration.rs`: child viewports are created without `init_accesskit`), and calls `App::ui` only while the root window is visible (`epi_integration.rs`, `if is_visible`). So the main window is the root (screen readers and UI Automation reach it) and the spotlight is a deferred child. Message pumping, the hotkey and the spotlight registration live in `App::logic`, which runs every pass even with every window hidden; with them in `ui`, the hotkey silently died whenever both windows were hidden.
  - *No transparent spotlight on Windows.* wgpu's default DX12 swapchain (from HWND) has no alpha; `Dx12SwapchainKind::DxgiFromVisual` renders transparent but over the window's GDI redirection surface, which paints white, because egui's `ViewportBuilder` cannot request `WS_EX_NOREDIRECTIONBITMAP`; the glow renderer logs "the GL config does not support it" on this NVIDIA driver. Resolution: the spotlight window *is* the card (560 wide, height fitted to content each frame); Windows 11 rounds and outlines its corners. macOS keeps a transparent window and a painted rounded card (unverified). eframe also reads the transparency flag for *all* surfaces from the root viewport, so the root sets it on macOS.
  - *Fonts.* egui 0.35's bundled fonts lack ▶ ✔ → 🤖 and many more. App chrome uses only covered glyphs or painter-drawn icons (`ui/icons.rs`), enforced by `tests/glyph_coverage.rs`, which scans every UI source file. Model output gets system fallbacks at runtime (Segoe UI, Segoe UI Symbol, Segoe UI Emoji; Helvetica Neue, Apple Symbols, Arial Unicode on macOS). CJK and complex scripts remain a known limitation.
  - *Deliberate behaviour differences from React* (all small, all tested): Stop bumps the run generation instead of setting a sticky `stopRequested` flag (React could swallow the next step's result after a Stop with nothing in flight); auto-executed command results carry the proposal's `tool_call_id` like approved ones; a main-window prompt sent before any Sync fetches the context and starts, instead of being silently ignored; merely viewing a session no longer re-saves it (React bumped its last-active time); the migration banner's cleanup uses an inline two-button choice instead of `confirm()`.
  - *Bugs found by the screenshot and headless checks, fixed:* avatars stretched across the row (assistant text wrapped one character per line); tool output and user bubbles didn't wrap long unbroken tokens and ran off-screen (`long_unbroken_text_stays_inside_the_window` now guards this); the sidebar row's click area swallowed the Rename/Delete buttons; `set_max_width(640)` widened the settings column past the window.
  - *Lockfile fragility.* Tauri pulls `windows` 0.61, and `gpu-allocator 0.28` accepts 0.53–0.62; if Cargo unifies it to 0.61, `wgpu-hal` fails to compile. Fixed with `cargo update -p gpu-allocator` (now on 0.62.2). Re-run that if a future `cargo update` breaks `wgpu-hal`.
  - *Tests.* `egui_kittest` harnesses crash the process (access violation in the GPU driver) when created concurrently; `tests/ui_screens.rs` serialises them with a mutex, so plain `cargo test` works.
  - *Smoke (Tier C).* `scripts/native-smoke.ps1` drives the real binary through text, command (approve via UI Automation), question (pick + answer), stop, and markdown scenarios, and checks the spotlight has no opaque margin. All five passed before the final layout-only fixes; the rerun after them was blocked because the Windows session got locked (`LogonUI.exe` running, input desktop inaccessible). Re-run it on an unlocked session.
  - *Unverified on this machine:* all macOS runtime behaviour (spotlight on all Spaces via `NSWindow.collectionBehavior`, template tray icon, transparency, Finder cursor-monitor centring, fonts), IME input, screen-reader output beyond the UIA tree.
- 2026-10-08 — **Phases 1–2 committed** (`5ef7316`), **Phase 3** (`b70760c`), follow-ups (`df44e6c`).
  - Windows: `build.rs` embeds `icons/icon.ico` and version info (checked by extracting the icon from the built exe); release builds use the GUI subsystem (PE header subsystem 2) and debug keeps the console (3). Release exe: 22.6 MB, single file, no WebView2.
  - macOS: `scripts/package-macos.sh` (lipo universal binary, `cdout.app`, ad-hoc signature with the Apple Events entitlement, `hdiutil` DMG) instead of cargo-packager: fewer moving parts, and every step is a standard Apple tool. Bundle id `io.cdout.native` while both apps ship; switch to `io.cdout.app` at Phase 4 so users keep their Finder grant. Script syntax-checked and plists validated with `plistlib` on Windows; the script itself only runs in the `native` release job on `macos-latest`.
  - `release.yml`: a `native` job after the Tauri one uploads `cdout-native-<v>-windows-x64.zip` and the DMG to the same draft release. `ci.yml`: runs the native integration tests (snapshots skipped in CI) and a clippy build with `fake-llm`.
  - Follow-ups: a hotkey that can't be registered at startup now shows a toast (the installed Tauri app holds Ctrl+Alt+A on the owner's machine); macOS dock click with every window hidden reopens the main window via `NSApplicationDidBecomeActiveNotification` (winit has no reopen event); smoke scenario `hidden` added.
  - **Smoke rerun on the owner's unlocked session (2026-10-08):** the first run failed 3 of 6 scenarios. Root cause: a real bug, not the test. Windows sometimes refuses the single activation request winit makes when the spotlight is shown (`focus_window` forces activation once), so the palette appeared without keyboard focus and typing went to the previous app. Fix: after showing, the spotlight re-requests focus every frame for up to 1.5 s until the OS reports it focused (`spotlight.focused` in the log; `spotlight.focus_failed` if it never does). The script now refuses to send any keystroke unless a cdout window owns the foreground, waits for `spotlight.focused`, and ignores the debug build's console window (which was the "window still visible after Close"). After the fix the full suite (text, command, question, stop, markdown, hidden) passed twice in a row.
  - Not covered end to end: tray menu items (tray menus are outside UI Automation's reach here), title-bar drag/maximise/minimise, the kill dialog against a real long-running command. Their state and screen logic is covered by unit and headless tests.

---

## Appendix A — reference spike (`docs/plans/native-egui-spike/`)
Verified on 2026-10-08. Copy patterns from it rather than from web tutorials:
- `Cargo.toml`: the exact eframe/egui_kittest feature sets that compiled and ran.
- `src/main.rs`: event-handler → `mpsc` → `request_repaint` wiring for tray, menu and hotkey; `ViewportBuilder` flags for the spotlight; `App::ui` with a transparent `CentralPanel` and a rounded card; `clear_color` returning transparent; hide/show via `ViewportCommand`.
- `tests/ui.rs`: minimal kittest test with click, re-run, label assertion and snapshot.
- `drive.ps1`: synthesized chord via `keybd_event`, `SendKeys`, screen capture with `CopyFromScreen`, process lifecycle with `try/finally`.
Build it standalone with `cargo build` inside that folder (it is not a workspace member). It uses `Ctrl+Alt+Shift+F9`.

## Appendix B — `egui_kittest` 0.35 Harness API (from docs.rs)
Constructors: `Harness::builder()`, `new_ui(FnMut(&mut Ui))`, `new_ui_state(FnMut(&mut Ui, &mut State), state)`, `new_eframe(FnOnce(&mut CreationContext) -> App)`. Builder: `with_size`, `with_pixels_per_point`, `with_options`, `with_max_steps`, `with_step_dt`, `build_ui`, `build_ui_state`.
Frames: `step()`, `run() -> u64`, `try_run()`, `run_ok()`, `run_steps(n)`, `set_size`, `set_pixels_per_point`.
Input: `key_press(Key)`, `key_down`, `key_up`, `key_press_modifiers(Modifiers, Key)`, `key_combination(&[Key])`, `event(Event)`, `hover_at`, `drag_at`, `drop_at`, `remove_cursor()`.
Queries (trait `egui_kittest::kittest::Queryable`): `get_by_label`, `query_by_label`, `get_by_role`, `query_by_role`, `get_by_role_and_label`, `get_by_label_contains`, `get_by_value`, `get_by(|node| ..)`, plus `get_all_*`/`query_all_*`; nodes support `.click()`.
State: `state()`, `state_mut()`, `into_state()`; the context is the public field `harness.ctx`.
Snapshots: `snapshot(name)`, `try_snapshot(name)`, `snapshot_options(name, &SnapshotOptions)`; the harness panics on drop if a snapshot mismatched; `UPDATE_SNAPSHOTS=true` rewrites baselines.

- 2026-10-18 — **Explorer selection fix (core, both shells).** Owner report: the selection cdout picks up is often not the one selected, "a lot when I have a bunch of tabs/windows". Investigated with `crates/cdout-core/examples/explorer_probe.rs` (kept; `--json` prints the resolver's diagnostics) and a controlled two-tab experiment driven by UI Automation. Two root causes, both in `features/explorer/windows.rs`:
  1. *The Explorer window was often not found at all.* The code walked the Z-order by hand with a 200-window cap, counting invisible windows; the owner's desktop has ~2,200 top-level windows and the first Explorer window sat at position ~1,350. Now `EnumWindows` (Z-order, no cap) picks the first `CabinetWClass` that is visible, not minimised and not DWM-cloaked (other virtual desktop); the foreground one wins if it is Explorer.
  2. *The wrong tab was chosen.* The active tab was matched by substring against the window title, with a fallback to "any tab with a selection". Explorer does not refresh a background window's title when its tab changes (observed: title "Screenshots and 7 more tabs" while the topmost tab was Downloads, which is the owner's screenshot), and substring matching with many tabs was ambiguous (duplicate names, prefixes). Verified signal: every tab owns a `ShellTabWindowClass` child, all visible with one rectangle, and Explorer keeps the *active tab's child topmost among its siblings*; this matched UI Automation's selected tab item on every switch. The resolver now uses that Z-order (`IServiceProvider` → `IShellBrowser::GetWindow` gives each `ShellWindows` entry its child window), falls back to a word-boundary title *prefix* match (longest name wins), then to the first tab, and never substitutes another tab's selection. Search-result views (no `file:` URL) report the selection with the first item's folder as the path. UNC `file://server/share` URLs are decoded.
  - Verification: 8 unit tests on the pure decision logic (stale title, longest prefix, duplicates, URL decoding); `scripts/explorer-selection-check.ps1` opens a real two-tab Explorer window, selects files through COM, switches tabs through UI Automation, and asserts the resolver's path and files for: each tab active, the window in the background while its tab changes, and an empty selection (reported empty, not borrowed) — passes. The diagnostics payload gained `frames`, `tabs` (with `z_index` and `is_active`), `resolution`, and `resolved_path`/`selected_count`/`selection`, so both shells' Diagnostics pages now show what was read on Windows too.
  - Not fixed here: Desktop selections (no Explorer window) and the macOS side, which was not reported.
