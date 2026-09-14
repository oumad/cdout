# cdout

A native desktop AI assistant for **Windows and macOS**. Press a global hotkey,
type "rename my photos by date" while files are selected in Explorer (or
Finder), and an LLM proposes a shell command for one-click approval.

Built with **Tauri v2 + React 19 + Rust**. Two windows: a small spotlight (600×260,
hotkey-triggered) and a main conversation window (collapsible sessions sidebar
on the left, like Claude.ai).

| | Windows | macOS |
|---|---|---|
| File context | Explorer, via COM (`IShellWindows`) | Finder, via AppleScript |
| Shell | PowerShell (`run_powershell` tool) | zsh (`run_shell` tool) |
| Default hotkey | `Ctrl+Alt+A` | `Cmd+Alt+A` |
| App data | `%APPDATA%\cdout` | `~/Library/Application Support/cdout` |

The shell is not abstracted behind a lowest-common-denominator wrapper — the
system prompt, the tool name, and the script templates all change with the
platform, so the model writes idiomatic PowerShell on Windows and idiomatic
zsh on macOS. See `src-tauri/src/platform.rs`.

---

## Quick start

### Run from source

```sh
# from repo root
npm install
npm run tauri dev
```

Prerequisites: Node 20+, a stable Rust toolchain, and the platform's native
webview toolchain — Visual Studio Build Tools + WebView2 on Windows, Xcode
Command Line Tools (`xcode-select --install`) on macOS.

**macOS, first launch:** cdout asks for permission to control Finder. That
prompt is how it reads your open folder and selected files; deny it and the
file context stays permanently empty (re-enable under System Settings →
Privacy & Security → Automation → cdout → Finder). Bundled skills also expect
their CLI tools on PATH — `brew install ffmpeg imagemagick exiftool` covers
three of the four.

### Run the test suites

```sh
# backend (Rust)
cd src-tauri
cargo test --lib

# frontend (TypeScript / React)
cd ..
npm test
```

### Configure providers

After first launch:

1. **OpenRouter (recommended)** — one key for ~400 models (Claude Opus 4.7,
   GPT-5, Gemini 3, Llama 4, etc.). Get a key at
   [openrouter.ai/keys](https://openrouter.ai/keys), paste it into Settings →
   Providers → "OpenRouter API Key". Save.
2. **Ollama (optional, local)** — if `ollama serve` is running on
   `localhost:11434`, your local models appear under "Local (Ollama)" in the
   picker. Change the URL in Settings if Ollama lives elsewhere.
3. **Anthropic direct (optional, for prompt caching)** — OpenRouter's
   OpenAI-compatible wire drops `cache_control` markers; if you want the 5–10×
   cost reduction from Anthropic's caching, paste an Anthropic API key in
   Settings as well. Models prefixed `anthropic:` then route directly.

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
- **Skills** — `*.md` files in `src-tauri/skills/` (bundled) or
  `<app data>/cdout/skills/` (user) get injected into the system prompt.
  Availability is probed against the real PATH — on macOS through a login
  shell, since a bundled `.app` otherwise cannot see Homebrew.
  User-supplied skills are scanned for unsafe patterns and quarantined if
  flagged.

---

## Architecture

### Backend (Rust — `src-tauri/src/`)

| File | Purpose |
|------|---------|
| `lib.rs` | Tauri command registration, tray, global shortcut, window lifecycle |
| `constants.rs` | Backend tunables (`MAX_OUTPUT_LEN`, `TOOL_NAME`, `COMPLETION_KEYWORDS`) |
| `llm/router.rs` | Three-branch dispatch: `ollama:*` → ollama, `anthropic:*` → direct Anthropic, else → OpenRouter |
| `llm/retry.rs` | Error classifier (`BusinessQuota` / `ContextWindowExceeded` / `EmptyCompletion` / `TransientRetryable` / `Terminal`) + retry wrapper with exp backoff + jitter |
| `llm/history.rs` | Context-overflow recovery: trim ladder (`fast_trim_tool_results` → `emergency_history_trim`) + orphan reconciliation |
| `llm/stream_util.rs` | Per-chunk 60s idle timeout + global cancel-flag check |
| `llm/mod.rs` | `Message`, `ToolCall`, `ToolDefinition`, `StreamChunk` types |
| `llm/clients/ollama.rs` | Local Ollama client (streaming, NDJSON) |
| `llm/clients/anthropic.rs` | Direct Anthropic API-key client (prompt caching + `compact-2026-01-12` beta for 4.6+ models) |
| `llm/clients/openrouter.rs` | OpenAI-compatible OpenRouter client. Pins `allow_fallbacks: false`, `quantizations: ["fp8","bf16","fp16"]`, `data_collection: "deny"` on every request |
| `features/agent.rs` | Agent loop, tool-proposal extraction (native `tool_calls` first, code-block + raw-text fallbacks), context-overflow recovery |
| `features/loop_detector.rs` | Process-wide singleton. Three patterns + four-state escalation. Pattern-keyed `warning_seen` so no-progress can reach Block/Break |
| `features/prompts.rs` | `PromptSection` trait, `sends_native_tool_specs` flag (`false` for `ollama:*` to get the longer prose tool catalog) |
| `features/skills.rs` | YAML-frontmatter `.md` loader, safety scanner with quarantine, `LoadedSkills { skills, quarantined }` return type |
| `features/sessions.rs` | Per-session JSON files in the app data dir |
| `features/explorer/mod.rs` | Shared `ExplorerState` + per-OS dispatch |
| `features/explorer/windows.rs` | Windows COM bridge to active Explorer window (`IShellWindows`) |
| `features/explorer/macos.rs` | Finder bridge via AppleScript (front window target + selection), with TCC-denial diagnosis |
| `platform.rs` | The only `cfg`-heavy file: shell invocation, process-group kill, binary detection, and the `ShellSpec` that feeds every shell-specific line of the system prompt |
| `tools/mod.rs` | `Tool` trait, `ToolRegistry`, `validate_and_execute`, cross-platform shell-tool aliases |
| `tools/shell.rs` | The shell exec tool (PowerShell / zsh). Tracks PID for kill, truncates output, and forwards error-shaped stderr even when the command exits 0 |
| `tools/risk.rs` | Allowlist command classifier (`ReadOnly` / `Mutating` / `Dangerous`) behind the approval policy |
| `tools/ask_user_question.rs` | The multi-choice clarification tool (validated client-side, never executed server-side) |
| `utils/config.rs` | `AppConfig` + `ApiKeys` (`{openrouter, anthropic}`) persistence |
| `utils/cancel.rs` | Global `OnceLock<AtomicBool>` cancel flag (single concurrent loop by design) |

### Frontend (TypeScript / React — `src/`)

| File / dir | Purpose |
|------------|---------|
| `App.tsx` | `SpotlightApp` + `MainApp`, routed by `getCurrentWindow().label` |
| `types/index.ts` | All cross-IPC types |
| `constants.ts` | Tauri command names, event names, magic numbers |
| `utils/tauri.ts` | Typed wrappers around every `invoke()` call (no raw invokes in components) |
| `utils/agent.ts` | `isTaskComplete` text-pattern check |
| `hooks/useModels.ts` | Model list reconciliation. On stale-slug detection, prefers `anthropic/*` → `openai/*` → `google/*` → `ollama:*` and toasts the swap |
| `hooks/useExplorer.ts` | File-manager state sync |
| `hooks/usePlatform.ts` | Host-OS labels + shell tool name, fetched from the backend so UI copy and the system prompt agree |
| `hooks/useError.ts` | Auto-dismissing error toast state |
| `hooks/useSessions.ts` | Session CRUD + debounced auto-save (500ms) |
| `components/SessionsSidebar.tsx` | Collapsible left sidebar with "New chat", session list, rename, delete |
| `components/ChatMessage.tsx` | Renders user / assistant / tool messages. Renders `synthetic: true` messages as a "cdout internal" gray note (not as user input) |
| `components/CommandApproval.tsx` | Editable command proposal with Approve / Approve-all / Reject-with-feedback / Dismiss |
| `components/QuestionApproval.tsx` | Radio/checkbox UI for `ask_user_question` with an "Other (free text)" fallback |
| `components/ModelSelector.tsx` | Provider-grouped `<optgroup>` (`Local (Ollama)` / `Anthropic (direct · cached)` / `OpenRouter`) |
| `components/MigrationBanner.tsx` | One-shot banner shown when legacy CLI/Antigravity/openai/gemini creds are detected. "Clean up legacy creds" button calls `cleanup_legacy_credentials` |
| `components/SettingsPage.tsx` | OpenRouter + Anthropic key fields (masked previews, never plaintext to renderer state), Ollama URL, free-tier toggle, quarantined-skill viewer, Diagnostics tab |
| `components/HotkeyRecorder.tsx` | Shows the current hotkey as keycaps and records a new chord. Canonicalises before comparing, so re-pressing the same combination in a different modifier order is not treated as a change |
| `components/FileAccessDiagnostics.tsx` | Explains why file context is empty. Distinguishes "permission denied" from "no window open", and deep-links the macOS Automation pane |

### Data flow — one agent turn

```
SpotlightApp                  Backend                        MainApp
─────────────                ─────────                       ────────
User types prompt
clicks Enter ───spotlight_submit──▶ persists prompt + model
                                    emits SPOTLIGHT_SUBMITTED ▶ listener:
                                                                  1) flush prior save
                                                                  2) hardResetSessionUi (cancel stream, clear UI)
                                                                  3) newSession (creates + loads)
                                                                  4) runAgentStep
                                                                ◀ run_agent_step_stream
                                    router::route_chat_stream
                                    classifier wraps errors
                                    LoopDetector records proposal
                                    AgentStepResult ───────────▶ frontend renders
                                                                  proposal → CommandApproval
                                                                  question → QuestionApproval
                                                                  text     → ChatMessage

                                                                User approves command
                                                            ──execute_shell_command──▶ PowerShell / zsh, capture
                                                                                        output, record outcome
                                                                                        in LoopDetector
                                                                ◀── tool_result
                                                                runAgentStep (next turn)
                                                                ...
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

- **Bundled** skills live in `src-tauri/skills/` and ship with the binary.
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

## Tauri commands (IPC surface)

| Command | Returns | Notes |
|---------|---------|-------|
| `get_explorer_status` | `ExplorerState` | Path + selected files of the active Explorer/Finder window |
| `get_explorer_debug` | `ExplorerDebugInfo` | For diagnosing detection failures. Shape is per-OS; on macOS it reports whether Automation access was granted |
| `get_platform_info` | `PlatformInfo` | OS key/name, file-manager name, shell name, shell tool name, file-list read hint |
| `get_approval_mode` / `set_approval_mode` | `ApprovalMode` | `ask` / `read_only` / `auto`, persisted. Written immediately on change, not on Settings-Save |
| `classify_command` | `CommandRisk` | `read_only` / `mutating` / `dangerous` for one command |
| `open_file_access_settings` | | macOS: deep-links System Settings → Privacy & Security → Automation. Errors on Windows, which has no grant to give |
| `get_ollama_models` | `Vec<String>` | All available model slugs, prefixed by provider |
| `get_ollama_url` / `set_ollama_url` | | |
| `get_selected_model` / `set_selected_model` | | |
| `set_openrouter_key` | | Plaintext input, server-side stored |
| `set_anthropic_key` | | |
| `get_api_keys` | `MaskedApiKeys` | `{ openrouter_set, openrouter_preview, anthropic_set, anthropic_preview }` — preview = last 4 chars |
| `get_show_free_openrouter_models` / `set_show_free_openrouter_models` | | Free tier hidden by default (FP4 quantization risk) |
| `get_openrouter_disclosure_ack` / `set_openrouter_disclosure_ack` | | One-shot migration banner ack |
| `has_legacy_credentials` | `bool` | Detects + auto-purges legacy `openai`/`gemini` fields from `api_keys.json` |
| `cleanup_legacy_credentials` | | Removes cdout's antigravity creds always; third-party CLI files only when `removeThirdParty: true` |
| `get_hotkey` | `String` | The currently registered chord |
| `set_hotkey` | | Re-registers with the OS **first**, and only writes to disk once that succeeds — so a chord another app owns cannot become the value loaded at next launch. Takes effect immediately, no restart |
| `init_agent_conversation` | `Vec<Message>` | Builds the system + initial user message. Takes optional `model` to switch native-tool-specs flag |
| `list_sessions` | `Vec<SessionMeta>` | Sidebar list, sorted by `last_active_at` desc |
| `load_session` / `create_session` / `save_session_messages` / `delete_session` / `rename_session` | | Session CRUD |
| `list_skills` | `{ skills, quarantined }` | |
| `run_agent_step` (sync) / `run_agent_step_stream` | `AgentStepResult` | Stream takes optional `drop_tools: bool` to honour `Break` verdict |
| `execute_shell_command` | `String` | Runs the proposed command in the platform shell. Records outcome in `LoopDetector` |
| `reset_loop_detector` | | Called on "New chat" and on spotlight resubmit |
| `cancel_stream` / `get_running_command` / `kill_running_command` | | Cancellation, process tracking, kill |
| `write_file_list` | `String` | Spills large file lists to a temp file the LLM can read (`Get-Content` / `read -r`) |
| `spotlight_submit` | | Spotlight → main window handshake. Carries `autoApprove` when submitted with the modifier held |

---

## Privacy notes

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

- Backend: **179 unit tests** at last count (`cargo test --lib`), run on both
  platforms — the suite is parameterised on `platform::SHELL`, not hardcoded
  to PowerShell.
  - `retry.rs` — every classifier boundary + the cross-bucket guards
  - `history.rs` — UTF-8 char-boundary safety, emergency floor, multi-pass orphan reconciliation, full-ladder happy path
  - `loop_detector.rs` — key-order-independent fingerprint, escalation through Warning/Block/Break for all 3 patterns, no-progress escapes Warning via pattern-keyed warning_seen, window eviction, singleton serialization via `test_lock`
  - `openrouter.rs` — provider-pin assertions, `data_collection:"deny"`, every `explain_failure` branch, mid-stream error frame surfacing, empty-stream `EmptyCompletion`, tool-call accumulation in order
  - `prompts.rs` — file-list block in all three size regimes, identity/rules section shortening on `sends_native_tool_specs`, the verify-before-complete rule, and a guard that the prompt never names the other platform's shell
  - `sessions.rs` — title generation (Unicode, truncation, empty), path-traversal rejection, create/load roundtrip, ordering by `last_active_at` desc, atomic write (tmp + rename), skip-corrupt-files, delete idempotency
  - `router.rs` — every prefix permutation including `anthropic:anthropic/...` and bare `anthropic/...`
  - `skills.rs` — every safety rule, case-insensitive matches, the curl-without-pipe-is-safe regression, wget-pipe-to-bash detection
  - `config.rs` — legacy `openai`/`gemini` field drop, default seed, data-dir
    migration (fast-path move, merge-without-clobber, stranded-state healing,
    fresh-install no-op)
  - `tools/*.rs` — registry lookup, cross-platform tool-name aliasing, a guard that no tool definition names the other platform's shell, `ask_user_question` validation, shell exec + failure exit codes, and the exit-0-with-errors stderr filter (including its char-boundary safety)
  - `platform.rs` — `{LIST_FILE}` template substitution, binary detection against the real PATH, tool-name/shell agreement
  - `risk.rs` — the allowlist policy from both directions: probes and read-only pipelines pass, writers/loops/unknown commands/backticks/redirection do not, substitution is judged recursively, and destructive patterns outrank a harmless-looking prefix
  - `features/explorer/macos.rs` — AppleScript payload parsing (folder vs file paths, desktop selections, filenames containing newlines), TCC-denial recognition
  - `agent.rs` — tool-proposal extraction, AgentStepResult serialization shape, loop-verdict escalation

- Frontend: **70 vitest tests** (`npm test`).
  - `agent.test.ts` — `isTaskComplete` patterns
  - `useError.test.ts` — show/clear/auto-dismiss timing
  - `useModels.test.ts` — model-list reconciliation + stale-slug swap
  - `QuestionApproval.test.tsx` — radio + multi-select + custom-answer paths
  - `ChatMessage.test.tsx` — synthetic vs real-user rendering distinction (the "cdout internal" badge)
  - `usePlatform.test.ts` — fallback before the backend answers, replacement after, and degradation when the IPC call fails
  - `FileAccessDiagnostics.test.tsx` — denial vs no-window-open vs working, the deep-link action, and that Windows is never offered a permission fix
  - `CommandApproval.test.tsx` — the destructive/writes/read-only badges, and that approve-once vs approve-all still differ
  - `HotkeyRecorder.test.tsx` — chord building and per-platform Meta mapping, refusal of modifier-less chords, Escape-cancels, surfacing an OS refusal without moving state, and canonical equality across modifier order
  - `ProviderSetup.test.tsx` — both onboarding paths including the remote-Ollama URL (save, trim, unchanged-URL no-op, error surfacing)

Run both via `cargo test --lib && npm test` from project root. CI runs the
same two suites plus `cargo clippy -D warnings` and `cargo fmt --check` on
Windows and macOS for every push and PR.

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
# Dev (hot-reload frontend, Rust recompiles on save)
npm run tauri dev

# Release build — NSIS + MSI on Windows, .app + .dmg on macOS
npm run tauri build

# macOS universal binary (Intel + Apple silicon), as CI ships it
npm run tauri build -- --target universal-apple-darwin
```

Output lands in `src-tauri/target/release/bundle/`. Tagging `v*` runs
`.github/workflows/release.yml`, which builds both platforms into one draft
release.

macOS builds are unsigned, so Gatekeeper blocks the first launch: right-click
→ **Open**, or `xattr -dr com.apple.quarantine /Applications/cdout.app`.
Signing and notarizing would need an Apple Developer ID in
`APPLE_CERTIFICATE` / `APPLE_ID` CI secrets — `entitlements.plist` is already
in place for it.

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

MIT. Personal-use desktop assistant — third-party provider terms apply (don't
ship cdout as a SaaS product without dealing with Anthropic / OpenAI /
Google ToS yourself).
