# cdout

A native desktop AI assistant for **Windows and macOS**. Press a global hotkey,
type "rename my photos by date" while files are selected in Explorer (or
Finder), and an LLM proposes a shell command for one-click approval.

Pure Rust, drawn with [egui](https://github.com/emilk/egui): no webview, no
Node, one binary. Two windows: a small hotkey-triggered spotlight card and a
main conversation window (collapsible sessions sidebar on the left, like
Claude.ai).

| | Windows | macOS |
|---|---|---|
| File context | Explorer, via COM (`IShellWindows`) | Finder, via AppleScript |
| Shell | PowerShell (`run_powershell` tool) | zsh (`run_shell` tool) |
| Default hotkey | `Ctrl+Alt+A` | `Cmd+Alt+A` |
| App data | `%APPDATA%\cdout` | `~/Library/Application Support/cdout` |

The shell is not abstracted behind a lowest-common-denominator wrapper — the
system prompt, the tool name, and the script templates all change with the
platform, so the model writes idiomatic PowerShell on Windows and idiomatic
zsh on macOS. See `crates/cdout-core/src/platform.rs`.

---

## Why this exists

I built cdout for myself. A lot of my work is the same batch operations over
and over — re-encoding clips, renaming a shoot by capture date, stripping
metadata, pulling frames — and I got tired of assembling another ffmpeg
incantation for a job I'd already done a dozen times. I wanted to describe
the outcome once and read back a command I could sanity-check.

It turned out useful enough day to day that it seemed worth putting out
there, in case anyone else works the same way. Fair warning that it's shaped
around how *I* work rather than around being a product: no account, no
telemetry, no update server, and the defaults reflect my own taste. If that
happens to suit you, help yourself.

---

## Quick start

### Download

Prebuilt binaries are on the [releases page](https://github.com/oumad/cdout/releases/latest):
a zip with `cdout.exe` for Windows, a universal DMG for macOS. Nothing to
install; the app lives in the tray / menu bar.

### Run from source

```sh
# from repo root
cargo run             # debug build
cargo run --release   # optimized; the binary is target/release/cdout(.exe)
```

Prerequisites: a stable Rust toolchain and the platform's C toolchain for the
linker — Visual Studio Build Tools (Desktop C++) on Windows, Xcode Command
Line Tools (`xcode-select --install`) on macOS.

**macOS, first launch:** cdout asks for permission to control Finder. That
prompt is how it reads your open folder and selected files; deny it and the
file context stays permanently empty (re-enable under System Settings →
Privacy & Security → Automation → cdout → Finder). Bundled skills also expect
their CLI tools on PATH — `brew install ffmpeg imagemagick exiftool` covers
three of the four.

### Run the test suites

```sh
cargo test --workspace
```

That covers the core's unit tests and the app's state-machine tests, headless
screen tests and glyph coverage. The scripts that drive the real binary are
under [Testing](#testing).

### Configure providers

Any one of these is enough; cdout picks up whatever is available at launch.

1. **A subscription you already have** — if [Claude Code](https://code.claude.com/docs/en/setup)
   or [Codex](https://github.com/openai/codex) is installed and signed in on
   the machine, there is nothing to configure: its models appear under "Your
   subscriptions" and cdout selects one. cdout runs the official CLI in
   headless mode with its own tools switched off, so your plan covers the
   usage and every command still waits for your approval. Not signed in yet?
   Run `claude` (or `codex login`) once in a terminal.
2. **Ollama (local)** — if `ollama serve` is running on `localhost:11434`,
   your local models appear under "Local (Ollama)". Change the URL in
   Settings if Ollama lives elsewhere.
3. **An API key** — an [OpenRouter](https://openrouter.ai/keys) key reaches
   Claude, GPT, Gemini and more, billed per request; an Anthropic API key
   adds direct `anthropic:` models with prompt caching (OpenRouter's
   OpenAI-compatible wire drops `cache_control`). Settings → Providers.

---

## What it does

- **Spotlight prompt** — global hotkey (`Ctrl+Alt+A` on Windows, `Cmd+Alt+A`
  on macOS; rebindable in Settings → General) opens a small command palette
  over any window. Type a request, hit
  Enter, the main window spawns/foregrounds and runs the agent in a fresh
  session.
- **File-manager-aware** — the agent sees the current Explorer/Finder window's
  path and selected files (Windows COM, or AppleScript on macOS). Prompts like
  "rename these" need no filename input.
- **One shell tool** — the single execution surface: `run_powershell` on
  Windows, `run_shell` (zsh) on macOS. Every proposed command is editable
  before approval. Output is fed back to the LLM for the next step.
- **Approval that isn't all-or-nothing** — by default, commands that provably
  cannot change anything (`ffprobe`, `ls`, `Test-Path`) run unattended, and
  anything that writes still waits for you. See below.
- **Ask-user tool** — when the request is ambiguous (rename pattern, output
  format, overwrite vs new file), the model can call `ask_user_question` and
  cdout renders a multiple-choice picker.
- **Sessions sidebar** — every spotlight prompt becomes a persistent session
  (auto-saved to `<app data>/cdout/sessions/<id>.json`). The sidebar on
  the left lists past chats by recency. Click to resume; rename or delete on
  hover.
- **Loop detection** — three patterns (exact-repeat, ping-pong, no-progress)
  with escalation Warning → Block → Break. Block pauses auto-execute; Break
  also drops tool definitions from the next request so the model is forced
  into a text-only reassessment.
- **Skills** — `*.md` files in `skills/` (bundled, compiled into the binary) or
  `<app data>/cdout/skills/` (user) get injected into the system prompt.
  Availability is probed against the real PATH — on macOS through a login
  shell, since a bundled `.app` otherwise cannot see Homebrew.
  User-supplied skills are scanned for unsafe patterns and quarantined if
  flagged.

---

## Architecture

Two crates in one Cargo workspace:

| Crate | What |
|---|---|
| `crates/cdout-core` | Everything that is not a pixel: agent loop, LLM clients, tools, approval policy, sessions, skills, config, file-manager bridges. No UI framework anywhere in it. |
| `crates/cdout` | The app: windows, tray, hotkey and the screens. Talks to the core through plain functions (`cdout_core::api`) and one streaming trait (`ChunkSink`). |

### Core (`crates/cdout-core/src/`)

| File | Purpose |
|------|---------|
| `api.rs` | The entry points the app calls: models and provider status, keys (masked on the way out), skills, session creation, `run_agent_step`, `execute_shell_command` |
| `constants.rs` | Backend tunables (`MAX_OUTPUT_LEN`, `TOOL_NAME`, `COMPLETION_KEYWORDS`) |
| `llm/router.rs` | Dispatch by prefix: `claude-code:*` / `codex:*` → the user's CLI, `ollama:*` → ollama, `anthropic:*` → direct Anthropic, else → OpenRouter |
| `llm/retry.rs` | Error classifier (`BusinessQuota` / `ContextWindowExceeded` / `EmptyCompletion` / `TransientRetryable` / `Terminal`) + retry wrapper with exp backoff + jitter |
| `llm/history.rs` | Context-overflow recovery: trim ladder (`fast_trim_tool_results` → `emergency_history_trim`) + orphan reconciliation |
| `llm/stream_util.rs` | Per-chunk 60s idle timeout + global cancel-flag check |
| `llm/mod.rs` | `Message`, `ToolCall`, `ToolDefinition`, `StreamChunk` types |
| `llm/clients/ollama.rs` | Local Ollama client (streaming, NDJSON) |
| `llm/clients/anthropic.rs` | Direct Anthropic API-key client (prompt caching + `compact-2026-01-12` beta for 4.6+ models) |
| `llm/clients/cli.rs` | Claude Code and Codex as providers: detection (`claude auth status`, `codex login status`), one headless run per step with the CLI's tools off, and a text tool protocol (a fenced `tool` block parsed back into native `tool_calls`, hidden from the stream) |
| `llm/clients/openrouter.rs` | OpenAI-compatible OpenRouter client. Pins `allow_fallbacks: false`, `quantizations: ["fp8","bf16","fp16"]`, `data_collection: "deny"` on every request |
| `llm/fake.rs` | Scripted model behind the `fake-llm` feature; what the end-to-end smoke test talks to |
| `features/agent.rs` | Agent loop, tool-proposal extraction (native `tool_calls` first, code-block + raw-text fallbacks), context-overflow recovery, the `ChunkSink` streaming trait |
| `features/loop_detector.rs` | Process-wide singleton. Three patterns + four-state escalation. Pattern-keyed `warning_seen` so no-progress can reach Block/Break |
| `features/prompts.rs` | `PromptSection` trait, `sends_native_tool_specs` flag (`false` for `ollama:*` to get the longer prose tool catalog) |
| `features/skills.rs` | YAML-frontmatter `.md` loader, safety scanner with quarantine, bundled skills embedded with `include_dir!` |
| `features/sessions.rs` | Per-session JSON files in the app data dir: list, load, save (atomic), rename, delete |
| `features/explorer/mod.rs` | Shared `ExplorerState` + per-OS dispatch |
| `features/explorer/windows.rs` | Windows COM bridge: finds Explorer windows with `EnumWindows`, picks the active tab by z-order (window titles of background windows are stale), reads its selection |
| `features/explorer/macos.rs` | Finder bridge via AppleScript (front window target + selection), with TCC-denial diagnosis |
| `platform.rs` | The only `cfg`-heavy file: shell invocation, process-group kill, binary detection, and the `ShellSpec` that feeds every shell-specific line of the system prompt |
| `tools/mod.rs` | `Tool` trait, `ToolRegistry`, `validate_and_execute`, cross-platform shell-tool aliases |
| `tools/shell.rs` | The shell exec tool (PowerShell / zsh). Tracks PID for kill, truncates output, and forwards error-shaped stderr even when the command exits 0 |
| `tools/risk.rs` | Allowlist command classifier (`ReadOnly` / `Mutating` / `Dangerous`) behind the approval policy |
| `tools/ask_user_question.rs` | The multi-choice clarification tool (validated client-side, never executed server-side) |
| `utils/config.rs` | `AppConfig` + `ApiKeys` (`{openrouter, anthropic}`) persistence, hotkey and approval-mode accessors |
| `utils/paths.rs` | The app data dir (`CDOUT_DATA_DIR` overrides it, which is how tests get an isolated run) |
| `utils/cancel.rs` | Global `OnceLock<AtomicBool>` cancel flag (single concurrent loop by design) |

### App (`crates/cdout/src/`)

| File / dir | Purpose |
|------------|---------|
| `state/` | The pure state machine: `AppState::update(Msg, now) -> Vec<Effect>`. Every branch of the UI logic lives here, with no I/O and injected time, so it is unit-tested directly (`state/tests.rs`) |
| `dispatch.rs` | Executes effects: async work on a tokio runtime, file I/O on a serial thread; results come back as messages. A generation counter drops results from a cancelled step |
| `app.rs` | The eframe `App`: message pump, main window (root viewport), spotlight (child viewport), focus and centring |
| `ui/` | Screens as functions of the state: spotlight, chat transcript (streaming markdown), command approval with risk badges, question picker, sidebar, settings, onboarding, title bar. `icons.rs` paints the icons the bundled fonts lack |
| `platform/` | The `cfg`-split window code: monitor under the cursor, activation, macOS menu-bar app behaviour |
| `hotkey.rs`, `tray.rs`, `fonts.rs` | Global shortcut (same chord syntax as the config), tray / menu-bar icon, system-font fallback |
| `crash.rs`, `logging.rs` | Panic hook that logs and relaunches the app (rate-limited, with the spotlight draft restored); file + stderr logging to `<app data>/logs/cdout.log` |

### Data flow — one agent turn

```
ui                       state machine                    dispatcher / core
──                       ─────────────                    ─────────────────
Enter in the spotlight ─▶ Msg::SpotlightSubmit
                          ├─ Effect::CreateSession ─────▶ api::create_session
                          ├─ Effect::ResetLoopDetector
                          └─ Effect::RunStep ───────────▶ api::run_agent_step (streams)
                                                          │ router → provider client
                                                          │ classifier wraps errors
                                                          │ LoopDetector records proposal
                          Msg::StreamDelta ◀──────────────┤ text as it arrives
                          Msg::StepDone    ◀──────────────┘ AgentStepResult
                          ├─ text      → transcript
                          ├─ question  → question picker
                          └─ proposal  → approval card, or straight to
                                          Effect::Execute if the policy allows
Approve ────────────────▶ Msg::ApproveCommand
                          └─ Effect::Execute ───────────▶ api::execute_shell_command
                          Msg::ExecDone    ◀──────────────┘ output, recorded in LoopDetector
                          └─ Effect::RunStep (next turn) …
```

---

## Approval policy

Approving every step is friction; approving nothing is reckless. Three modes,
in Settings → Approval:

| Mode | Behaviour |
|------|-----------|
| Ask every time | Nothing runs without a click |
| **Auto-run read-only** (default) | Provably read-only commands run unattended; anything that writes waits |
| Auto-run everything | Executes whatever it proposes |

`tools/risk.rs` does the classification, and it is an **allowlist**: a command
is read-only only if every segment of it names a known read-only tool, with no
surviving redirection, no backticks, and command substitution only when the
substituted command is itself read-only. Anything unrecognised — including
loops and subshells, even when their body is a probe — counts as mutating.
A gap in the list therefore costs an extra approval prompt, never an unwanted
execution.

Measured against the commands a local 27B actually produced during
evaluation, 6 of 10 auto-ran and **every** `ffmpeg` write required approval.

Two things are not configurable:

- **Recognisably destructive commands always stop for approval** — `rm -rf /`,
  `sudo`, `mkfs`, `dd if=`, `csrutil disable` and friends — even in
  "auto-run everything". The approval card marks them in red.
- Loop detection, the consecutive-failure pause and the step cap apply in
  every mode.

For a single task, `Cmd+Enter` (`Ctrl+Enter` on Windows) in the spotlight runs
it unattended without changing the setting — so you never have to wait for the
first proposal just to click **All**. While a run is pre-authorised the
toolbar shows an **Auto** badge that clicks back to manual.

Classification lives in the backend next to the tool that executes the
command: a renderer-side allowlist would be one XSS away from advisory.

---

## Why a successful command can still report errors

`[Exit code: 0 — Success]` is not proof of success, and the system prompt says
so (rule 5). For that instruction to be followable, the tool result has to
carry the evidence: a partially corrupt input makes ffmpeg emit hundreds of
`Invalid NAL unit size` lines, write a damaged file, and **still exit 0**.

So on success, `tools/shell.rs` forwards only the *error-shaped* lines from
stderr (`ERROR_MARKERS`), capped at 20 and keeping the tail — where the fatal
one usually is. ffmpeg's banner, stream table and progress output are all
stderr too, and none of it reaches the model. On failure, stderr is forwarded
in full, unchanged.

---

## Loop detection & recovery semantics

- **Exact-repeat** — same canonical `(tool_name, args)` fingerprint 3× in last
  6 turns → escalates.
- **Ping-pong** — two distinct fingerprints alternating 3× → escalates.
- **No-progress** — same tool called 4× with at least 4 recorded failures and
  zero successes → escalates. Pattern-keyed escalation so drifting args still
  reach Block/Break.

States: `Ok` → `Warning` (model nudged, auto-execute continues) → `Block`
(auto-execute pauses, user must approve next command) → `Break` (auto-execute
pauses AND next request drops tool definitions, forcing text-only
reassessment).

Context overflow is handled separately by the retry classifier — see
`llm/history.rs`. The trim ladder is:
`fast_trim_tool_results` → `emergency_history_trim` →
`remove_orphaned_tool_messages` (4 passes; Pass 3 drops empty-assistant
messages so Anthropic doesn't 400 on them).

---

## Skills

`.md` files with YAML frontmatter:

```yaml
---
name: ffmpeg
description: Video processing toolkit
requires:
  bins: [ffmpeg]
---

Use `ffmpeg -i input.mp4 -vf scale=1280:720 out.mp4` to resize.
```

- **Bundled** skills live in `skills/` and are compiled into the binary.
  They are trusted — NOT run through the safety scanner. Treat any PR that
  adds a bundled skill as a security review.
- **User** skills live in `<app data>\skills\` (`%APPDATA%\cdout\skills\`,
  `~/Library/Application Support/cdout/skills/`) and ARE scanned. Hits
  on prompt-injection wording (`ignore previous instructions`,
  `<system>`, `you are now`), destructive shell (`rm -rf /`, `chmod 777`,
  `format c:`, `dd if=/dev/zero`), or pipe-to-shell (`curl ... | sh`,
  `wget ... | bash`) → the skill is quarantined and NOT injected into the
  prompt. Settings → Skills shows the quarantined list with hit details.

Bundled skills currently: `ffmpeg`, `oiiotool`, `imagemagick`, `exiftool`.

---

## Provider routing

```
model id pattern              dispatch                        rationale
─────────────────             ────────                        ─────────
claude-code:sonnet            `claude -p` (Claude Code)       the user's own plan, no key
codex:default                 `codex exec` (Codex)            the user's own plan, no key
ollama:llama3                 ollama (localhost)              local, private, free
anthropic:claude-opus-4-7     direct Anthropic API            prompt caching works
anthropic/claude-opus-4.7     OpenRouter                      same model, no caching
openai/gpt-5                  OpenRouter
google/gemini-3-pro-preview   OpenRouter
anything else                 OpenRouter (default)
```

`route_provider` strips an optional inner `anthropic/` prefix from
`anthropic:` so `anthropic:anthropic/claude-opus-4-7` also works.

---

## Configuration files

All paths are relative to the app data dir: `%APPDATA%\cdout` on Windows,
`~/Library/Application Support/cdout` on macOS (`dirs::config_dir()`).

| Path | Purpose |
|------|---------|
| `cdout_config.json` | Ollama URL, hotkey, selected model, `show_free_openrouter_models`, `openrouter_disclosure_ack` |
| `api_keys.json` | `{ openrouter, anthropic }` plaintext (server-side only — never returned to renderer; masked `…3fa1` preview is sent instead) |
| `sessions/<id>.json` | One file per session — meta + full message history. Sidebar parses meta only |
| `skills/*.md` | User-supplied skills (scanned + quarantined as needed) |

On first launch after the shuttle-io → cdout rename, the app moves
`%APPDATA%\shuttle-io\` to `%APPDATA%\cdout\` (and `shuttle_config.json` to
`cdout_config.json`) automatically, so keys, sessions, and user skills carry
over. See `config::migrate_legacy_data_dir`.

---

## Core API

The app does not reach into the core's modules for behaviour; it calls
`cdout_core::api`:

| Function | Notes |
|----------|-------|
| `platform_info` | OS key/name, file-manager name, shell name, shell tool name, file-list read hint |
| `available_models` | Every usable model slug, prefixed by provider |
| `provider_status` | Whether Ollama answers and which keys are set; drives first-run onboarding |
| `set_ollama_url`, `set_openrouter_key`, `set_anthropic_key` | Plaintext in, stored on disk. `masked_api_keys` is the only way back out (set flag + last 4 chars) |
| `has_legacy_credentials`, `cleanup_legacy_credentials` | Detect and remove credential layouts from older versions |
| `list_skills` | Loaded skills plus the quarantined ones with their hits |
| `init_agent_conversation` | System prompt + first user message. Takes the model, to switch the native-tool-specs flag |
| `create_session`, `save_session_messages` | Session files. Listing, loading, renaming and deleting are in `features::sessions` |
| `run_agent_step` | One streaming LLM step; chunks go to a `ChunkSink`. `drop_tools` honours the loop detector's `Break` verdict |
| `execute_shell_command` | Runs an approved command in the platform shell and records the outcome in the `LoopDetector` |
| `write_file_list` | Spills large file lists to a temp file the model reads with `Get-Content` / `read -r` |
| `open_file_access_settings` | macOS: deep-links System Settings → Privacy & Security → Automation. Errors on Windows, which has no grant to give |

Plain settings (hotkey, approval mode, selected model, Ollama URL, free-tier
toggle) go through `utils::config`; the file-manager bridge through
`features::explorer`; the command classifier through `tools::risk`;
cancellation and the kill switch through `utils::cancel` and `tools::shell`.
`set_hotkey` re-registers with the OS **first** and only writes to disk once
that succeeds, so a chord another app owns can never become the value loaded
at next launch.

---

## Privacy notes

- **Subscription path (Claude Code, Codex)** — cdout runs the official CLI
  and the CLI talks to Anthropic or OpenAI under your account, as it would
  in a terminal. cdout never reads, stores or forwards the CLI's
  credentials. Each step runs in an empty working directory with the CLI's
  own tools off (Claude Code) or in a read-only sandbox (Codex), and no
  session is saved on the CLI's side.
- **Ollama path** — fully local. Prompts never leave the machine.
- **Anthropic direct path** — prompts go straight to `api.anthropic.com` with
  your key. No cdout intermediary.
- **OpenRouter path** — prompts go to `openrouter.ai`, which forwards to the
  upstream provider. cdout pins `data_collection: "deny"` and
  `allow_fallbacks: false` on every request. OpenRouter still logs metadata
  by default; their training-opt-out is configured on the OpenRouter
  dashboard side. The migration banner discloses this in-product.

---

## Testing

Every layer has a test that runs without a human at the keyboard and reports
in a form a script (or an agent) can read; that is how the app is developed.

- **Core unit tests** — `cargo test -p cdout-core`, run on both platforms;
  the suite is parameterised on `platform::SHELL`, not hardcoded to
  PowerShell.
  - `retry.rs` — every classifier boundary + the cross-bucket guards
  - `history.rs` — UTF-8 char-boundary safety, emergency floor, multi-pass orphan reconciliation, full-ladder happy path
  - `loop_detector.rs` — key-order-independent fingerprint, escalation through Warning/Block/Break for all 3 patterns, no-progress escapes Warning via pattern-keyed warning_seen, window eviction, singleton serialization via `test_lock`
  - `openrouter.rs` — provider-pin assertions, `data_collection:"deny"`, every `explain_failure` branch, mid-stream error frame surfacing, empty-stream `EmptyCompletion`, tool-call accumulation in order
  - `prompts.rs` — file-list block in all three size regimes, identity/rules section shortening on `sends_native_tool_specs`, the verify-before-complete rule, and a guard that the prompt never names the other platform's shell
  - `sessions.rs` — title generation (Unicode, truncation, empty), path-traversal rejection, create/load roundtrip, ordering by `last_active_at` desc, atomic write (tmp + rename), skip-corrupt-files, delete idempotency
  - `router.rs` — every prefix permutation including `anthropic:anthropic/...` and bare `anthropic/...`
  - `clients/cli.rs` — sign-in status parsing for both CLIs, their stream formats (recorded from real runs), tool-block parsing and its leniencies, the stream filter that hides tool JSON at every possible split point, transcript rendering round-trips
  - `skills.rs` — every safety rule, case-insensitive matches, the curl-without-pipe-is-safe regression, wget-pipe-to-bash detection
  - `config.rs` — legacy `openai`/`gemini` field drop, default seed, data-dir
    migration (fast-path move, merge-without-clobber, stranded-state healing,
    fresh-install no-op)
  - `tools/*.rs` — registry lookup, cross-platform tool-name aliasing, a guard that no tool definition names the other platform's shell, `ask_user_question` validation, shell exec + failure exit codes, and the exit-0-with-errors stderr filter (including its char-boundary safety)
  - `platform.rs` — `{LIST_FILE}` template substitution, binary detection against the real PATH, tool-name/shell agreement
  - `risk.rs` — the allowlist policy from both directions: probes and read-only pipelines pass, writers/loops/unknown commands/backticks/redirection do not, substitution is judged recursively, and destructive patterns outrank a harmless-looking prefix
  - `features/explorer/windows.rs` — the pure parts of tab resolution: title-prefix matching at word boundaries, topmost-tab selection, `file:///` URL decoding
  - `features/explorer/macos.rs` — AppleScript payload parsing (folder vs file paths, desktop selections, filenames containing newlines), TCC-denial recognition
  - `agent.rs` — tool-proposal extraction, AgentStepResult serialization shape, loop-verdict escalation

- **App unit tests** — `cargo test -p cdout --lib`: the state machine in
  `state/tests.rs`, with injected time. Approval policy matrix, auto-steps and
  their cap, three-failures pause, loop Block/Break, prompts queued while a
  step runs, stale results from a stopped step, spotlight blur grace and the
  blur-before-first-focus case, debounced saves, session switching, draft
  restore after a crash.

- **Headless screen tests** — `cargo test -p cdout --tests`: every screen
  rendered off-screen with `egui_kittest`, queried through its accessibility
  tree (a test clicks "Approve" and checks what appears) and compared to the
  pixel snapshots in `crates/cdout/tests/snapshots/`. Regenerate on purpose
  with `UPDATE_SNAPSHOTS=true`; CI skips the pixel comparison
  (`CDOUT_SKIP_SNAPSHOTS=1`) because its GPUs render slightly differently.
  `tests/glyph_coverage.rs` fails if a UI string uses a character the bundled
  fonts cannot draw — that is why the chrome paints its own icons.

- **End-to-end smoke test** — `scripts/native-smoke.ps1` (Windows): builds
  the app with the `fake-llm` feature, launches it against a throwaway data
  dir and a hotkey that cannot clash with an installed cdout, then drives it
  with real key presses and UI Automation through seven scenarios (text
  answer, command approval, question, stop/kill, markdown, hidden-window
  behaviour, crash and relaunch). Every step is checked against the app's log;
  failures print the log tail. Needs an unlocked desktop session.

- **Explorer detection check** — `scripts/explorer-selection-check.ps1`
  (Windows): opens a real two-tab Explorer window, switches tabs through UI
  Automation and asserts the resolver sees the right folder and selection,
  including the stale-title case. `cargo run -p cdout-core --example
  explorer_probe -- --json` dumps what the resolver sees right now.

- **Subscription backends** — `cargo run -p cdout-core --example cli_probe`
  (any OS): reports what detection sees, then drives each signed-in CLI
  through the real agent loop: a request that needs a command, a follow-up
  after a successful result, and an ambiguous request that must come back as
  a question. Fails if raw tool JSON reaches the streamed text. Uses a little
  of your plan's quota; pass a model (`claude-code:haiku`) to probe just one.

CI runs `cargo test --workspace`, clippy (with and without `fake-llm`) and
`cargo fmt --check` on Windows and macOS for every push and PR.

---

## Evaluating a local model

Local models vary wildly at tool-calling, which is the only thing that matters
here. `dev/eval_local_model.py` runs a model through cdout's actual loop —
same system prompt, same tool schemas, same tool-result format, same step cap
— against a deliberately awkward clip set (spaced filename, uppercase
extension, mixed resolutions, one clip with no audio) and five tasks including
a crossfaded reel and a corrupt file.

```sh
# dump the prompt the app really sends, then evaluate against it
OLLAMA_URL=http://192.168.1.50:11434 python3 dev/eval_local_model.py qwen3.8:27b
```

It auto-generates its fixtures, executes what the model proposes in a scratch
directory behind a denylist, and asserts that no input file was destroyed or
modified. Full per-step transcripts land in `results.json`.

Prompt edits produce run-to-run variance that easily swamps their real effect,
so before concluding anything from a single pass, repeat one scenario:

```sh
EVAL_ONLY=s1_convert_720p EVAL_REPEAT=5 python3 dev/eval_local_model.py qwen3.8:27b
```

Note it auto-approves commands, which cdout itself never does — a human
always clicks Approve. Point it at a scratch directory only.

---

## Build & ship

```sh
# Windows: one exe with the icon embedded, no console window
cargo build --release            # target/release/cdout.exe

# macOS: universal (Intel + Apple silicon) .app and DMG, ad-hoc signed
scripts/package-macos.sh         # dist/cdout-<version>-universal.dmg
```

The version lives once, in the root `Cargo.toml` (`[workspace.package]`).
Tagging `v*` runs `.github/workflows/release.yml`, which builds both
platforms into one draft release.

macOS builds are unsigned, so Gatekeeper blocks the first launch: right-click
→ **Open**, or `xattr -dr com.apple.quarantine /Applications/cdout.app`.
Signing and notarizing would need an Apple Developer ID;
`crates/cdout/macos/entitlements.plist` is already in place for it.

---

## Troubleshooting

**macOS: the file count stays at 0 / no folder is shown.** Automation
permission was denied. Settings → **Diagnostics** says so outright and
deep-links the fix; or do it by hand under System Settings → Privacy &
Security → Automation → cdout → Finder. The consent dialog appears only once,
so a missed or denied prompt never comes back on its own.

**macOS: "cdout is damaged and can't be opened" / "unidentified developer".**
The build is unsigned. Right-click the app → **Open**, or
`xattr -dr com.apple.quarantine /Applications/cdout.app`.

**macOS: every skill shows as Missing.** The bundled skills need their CLI
tools on PATH, and a `.app` launched from Finder inherits only
`/usr/bin:/bin:/usr/sbin:/sbin`. cdout runs commands through a login shell and
seeds the Homebrew prefixes to compensate, so this usually means the tool
genuinely is not installed — check with `which ffmpeg` in a terminal.
Settings → Skills lists exactly which binaries are missing.

**The app vanished while I was typing, then came back.** It crashed and
relaunched itself: the prompt you were typing is restored and a notice says
so. The panic is in `<app data>/logs/cdout.log`. On Windows the known cause
is the graphics driver losing the device when another process fills the GPU's
memory; cdout uses DirectX 12 there, which survives that, and
`WGPU_BACKEND=vulkan|dx12|metal|gl` overrides the choice if you need to.

**Claude Code or Codex does not show up in the picker.** Settings →
Providers → Subscriptions says what cdout found: not installed, installed but
not signed in (run `claude` or `codex login` once in a terminal), or signed in.
cdout looks on PATH and in the usual install folders; a CLI installed
somewhere else needs its folder on PATH.

**The hotkey does nothing.** Another app has claimed the chord — the OS gives
no feedback when it refuses a registration. Settings → **General** shows the
current hotkey and records a new one: press the keys you want, and if the OS
refuses, cdout says so and keeps the previous binding. A corrupt value on
disk falls back to the built-in default rather than bricking startup, and the
spotlight is always reachable from the tray/menu bar regardless.

**A local model writes commands but never runs them.** It is not emitting
native tool calls. cdout has text-extraction fallbacks, but they are a
safety net — see the eval harness above to check a model properly before
relying on it.

**A command "succeeded" but the output file is wrong.** Exit 0 does not mean
no errors; see the section above. If a tool wrote errors to stderr, the tool
result now says so explicitly.

---

## Known limitations / pending work

- **Small Ollama models** still hallucinate "Task Complete" without verifying
  output files. The system prompt forces an existence check now (rule 5 —
  `Test-Path` on Windows, `test -f` on macOS), but weak models will sometimes
  still claim success. Lean on Claude/GPT/Gemini
  via OpenRouter for anything destructive.
- **Spotlight always starts a fresh session.** If you want to continue an
  existing conversation, switch to the main window and pick the session in
  the sidebar first.
- **Subscription steps are slower than an API.** Each step starts the CLI
  afresh (a few seconds for Claude Code). Codex follows your Codex config's
  model and reasoning effort; a high effort can take half a minute per step.
- **Gemini CLI and Grok are not wired up yet.** The CLI backend is generic
  enough to add them; each needs its headless output format checked on a
  signed-in machine first.
- **OpenRouter outage recovery is manual.** No auto-failover to direct
  Anthropic — we surface a clear "service issue, not your key" error
  instead.
- **Single concurrent agent loop by design.** The global cancel flag and
  `LoopDetector` singleton assume one in-flight conversation. Subagents /
  multi-window concurrent loops would need a per-session `CancellationToken`.
- **Multimodal not yet wired.** Vision / image input would need a new
  `MessagePart` enum and per-client serialization.
- **macOS builds are unsigned**, so every download needs the Gatekeeper
  right-click dance, and Finder access needs a one-time Automation grant.
- **Linux is not supported.** It compiles (the shell path is shared with
  macOS) but there is no file-manager integration — `explorer/unsupported.rs`
  returns an empty context, so prompts must name their paths.

---

## Contributing

Issues and pull requests are welcome — including ones your agent wrote, which
would be fitting enough. If something's broken, or there's a tool or workflow
you wish it handled, open an issue or just send the patch.

A couple of things that make a PR easy to take:

- `cargo test --workspace` should pass. CI runs it plus clippy and
  `cargo fmt --check` on Windows *and* macOS for every pull request — and
  that matrix is the only thing that compiles both halves of the platform
  split, since half the backend is `cfg`-ed out on any single machine.
- Anything touching command execution or the approval policy wants a test.
  That code decides what runs on someone's files without being asked twice.
- UI logic goes in the state machine (`crates/cdout/src/state/`), where it
  can be unit-tested with injected time; screens stay functions of the state,
  so a headless screen test can cover them.
- Platform-specific work belongs behind `crates/cdout-core/src/platform.rs`
  (shell) and `crates/cdout/src/platform/` (windows) rather than sprinkled
  `cfg!` blocks, so the prompt and the shell can't drift apart.

If you're adding a skill, note that user skills are scanned and bundled ones
are not — a PR that ships a bundled skill is a security review, so say what
the CLI does and why it's safe to hand a model.

---

## Lineage

cdout continues [**proton-io**](https://oumad.github.io/proton-io/) — *"a
shortcut to any task"* — a Windows launcher where you wrote the scripts
yourself, gave them hotkeys, and picked one from a searchable menu. It already
had the two ideas this is built on: a global hotkey over any window, and
scripts that receive the current directory and selection straight from the
file manager.

What changed is *who writes the script*. In proton-io you built each task up
front in a script builder, wiring typed inputs — text fields, file pickers,
dropdowns, sliders — into a command. In cdout you describe the outcome in a
sentence and the model proposes the command, which you read and approve. The
parameter pickers became `ask_user_question`; the script library became
skills; the Windows-only COM context grew a Finder counterpart.

---

## License

MIT — see [LICENSE](LICENSE). Third-party provider terms still apply on top:
don't ship cdout as a SaaS product without dealing with the Anthropic /
OpenAI / Google / OpenRouter terms yourself.
