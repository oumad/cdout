# shuttle-io

A Windows-native AI assistant. Press a global hotkey, type "rename my photos by
date" while files are selected in Explorer, and an LLM proposes a PowerShell
command for one-click approval.

Built with **Tauri v2 + React 19 + Rust**. Two windows: a small spotlight (600×260,
hotkey-triggered) and a main conversation window (collapsible sessions sidebar
on the left, like Claude.ai).

---

## Quick start

### Run from source

```powershell
# from repo root
npm install
npm run tauri dev
```

### Run the test suites

```powershell
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

- **Spotlight prompt** — global hotkey (default `Ctrl+Alt+A`) opens a small
  command palette over any window. Type a request, hit Enter, the main window
  spawns/foregrounds and runs the agent in a fresh session.
- **Explorer-aware** — the agent sees the current Explorer window's path and
  selected files (via Windows COM). Prompts like "rename these" need no
  filename input.
- **PowerShell tool** — the single execution surface. Every proposed command
  is editable before approval. Output is fed back to the LLM for the next
  step.
- **Ask-user tool** — when the request is ambiguous (rename pattern, output
  format, overwrite vs new file), the model can call `ask_user_question` and
  shuttle-io renders a multiple-choice picker.
- **Sessions sidebar** — every spotlight prompt becomes a persistent session
  (auto-saved to `%APPDATA%\shuttle-io\sessions\<id>.json`). The sidebar on
  the left lists past chats by recency. Click to resume; rename or delete on
  hover.
- **Loop detection** — three patterns (exact-repeat, ping-pong, no-progress)
  with escalation Warning → Block → Break. Block pauses auto-execute; Break
  also drops tool definitions from the next request so the model is forced
  into a text-only reassessment.
- **Skills** — `*.md` files in `src-tauri/skills/` (bundled) or
  `%APPDATA%\shuttle-io\skills\` (user) get injected into the system prompt.
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
| `features/sessions.rs` | Per-session JSON files in `%APPDATA%\shuttle-io\sessions\` |
| `features/explorer.rs` | Windows COM bridge to active Explorer window (`IShellWindows`) |
| `tools/mod.rs` | `Tool` trait, `ToolRegistry`, `validate_and_execute` |
| `tools/powershell.rs` | The PowerShell exec tool. Tracks PID for kill, truncates output |
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
| `hooks/useExplorer.ts` | Explorer state sync |
| `hooks/useError.ts` | Auto-dismissing error toast state |
| `hooks/useSessions.ts` | Session CRUD + debounced auto-save (500ms) |
| `components/SessionsSidebar.tsx` | Collapsible left sidebar with "New chat", session list, rename, delete |
| `components/ChatMessage.tsx` | Renders user / assistant / tool messages. Renders `synthetic: true` messages as a "shuttle internal" gray note (not as user input) |
| `components/CommandApproval.tsx` | Editable PowerShell proposal with Approve / Approve-all / Reject-with-feedback / Dismiss |
| `components/QuestionApproval.tsx` | Radio/checkbox UI for `ask_user_question` with an "Other (free text)" fallback |
| `components/ModelSelector.tsx` | Provider-grouped `<optgroup>` (`Local (Ollama)` / `Anthropic (direct · cached)` / `OpenRouter`) |
| `components/MigrationBanner.tsx` | One-shot banner shown when legacy CLI/Antigravity/openai/gemini creds are detected. "Clean up legacy creds" button calls `cleanup_legacy_credentials` |
| `components/SettingsPage.tsx` | OpenRouter + Anthropic key fields (masked previews, never plaintext to renderer state), Ollama URL, free-tier toggle, quarantined-skill viewer |

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
                                                                ────execute_powershell────▶ PowerShell, capture output,
                                                                                            record outcome in LoopDetector
                                                                ◀── tool_result
                                                                runAgentStep (next turn)
                                                                ...
```

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
- **User** skills live in `%APPDATA%\shuttle-io\skills\` and ARE scanned. Hits
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

| Path | Purpose |
|------|---------|
| `%APPDATA%\shuttle-io\shuttle_config.json` | Ollama URL, hotkey, selected model, `show_free_openrouter_models`, `openrouter_disclosure_ack` |
| `%APPDATA%\shuttle-io\api_keys.json` | `{ openrouter, anthropic }` plaintext (server-side only — never returned to renderer; masked `…3fa1` preview is sent instead) |
| `%APPDATA%\shuttle-io\sessions\<id>.json` | One file per session — meta + full message history. Sidebar parses meta only |
| `%APPDATA%\shuttle-io\skills\*.md` | User-supplied skills (scanned + quarantined as needed) |

---

## Tauri commands (IPC surface)

| Command | Returns | Notes |
|---------|---------|-------|
| `get_explorer_status` | `ExplorerState` | Path + selected files of active Explorer window |
| `get_explorer_debug` | `ExplorerDebugInfo` | For diagnosing Explorer detection |
| `get_ollama_models` | `Vec<String>` | All available model slugs, prefixed by provider |
| `get_ollama_url` / `set_ollama_url` | | |
| `get_selected_model` / `set_selected_model` | | |
| `set_openrouter_key` | | Plaintext input, server-side stored |
| `set_anthropic_key` | | |
| `get_api_keys` | `MaskedApiKeys` | `{ openrouter_set, openrouter_preview, anthropic_set, anthropic_preview }` — preview = last 4 chars |
| `get_show_free_openrouter_models` / `set_show_free_openrouter_models` | | Free tier hidden by default (FP4 quantization risk) |
| `get_openrouter_disclosure_ack` / `set_openrouter_disclosure_ack` | | One-shot migration banner ack |
| `has_legacy_credentials` | `bool` | Detects + auto-purges legacy `openai`/`gemini` fields from `api_keys.json` |
| `cleanup_legacy_credentials` | | Removes shuttle-io's antigravity creds always; third-party CLI files only when `removeThirdParty: true` |
| `get_hotkey` / `set_hotkey` | | |
| `init_agent_conversation` | `Vec<Message>` | Builds the system + initial user message. Takes optional `model` to switch native-tool-specs flag |
| `list_sessions` | `Vec<SessionMeta>` | Sidebar list, sorted by `last_active_at` desc |
| `load_session` / `create_session` / `save_session_messages` / `delete_session` / `rename_session` | | Session CRUD |
| `list_skills` | `{ skills, quarantined }` | |
| `run_agent_step` (sync) / `run_agent_step_stream` | `AgentStepResult` | Stream takes optional `drop_tools: bool` to honour `Break` verdict |
| `execute_powershell` | `String` | Runs the proposed PowerShell. Records outcome in `LoopDetector` |
| `reset_loop_detector` | | Called on "New chat" and on spotlight resubmit |
| `cancel_stream` / `get_running_command` / `kill_running_command` | | Cancellation, process tracking, kill |
| `write_file_list` | `String` | Spills large file lists to a temp file the LLM can `Get-Content` |
| `spotlight_submit` | | Spotlight → main window handshake |

---

## Privacy notes

- **Ollama path** — fully local. Prompts never leave the machine.
- **Anthropic direct path** — prompts go straight to `api.anthropic.com` with
  your key. No shuttle-io intermediary.
- **OpenRouter path** — prompts go to `openrouter.ai`, which forwards to the
  upstream provider. shuttle-io pins `data_collection: "deny"` and
  `allow_fallbacks: false` on every request. OpenRouter still logs metadata
  by default; their training-opt-out is configured on the OpenRouter
  dashboard side. The migration banner discloses this in-product.

---

## Testing

- Backend: **128 unit tests** at last count (`cargo test --lib`).
  - `retry.rs` — every classifier boundary + the cross-bucket guards
  - `history.rs` — UTF-8 char-boundary safety, emergency floor, multi-pass orphan reconciliation, full-ladder happy path
  - `loop_detector.rs` — key-order-independent fingerprint, escalation through Warning/Block/Break for all 3 patterns, no-progress escapes Warning via pattern-keyed warning_seen, window eviction, singleton serialization via `test_lock`
  - `openrouter.rs` — provider-pin assertions, `data_collection:"deny"`, every `explain_failure` branch, mid-stream error frame surfacing, empty-stream `EmptyCompletion`, tool-call accumulation in order
  - `prompts.rs` — file-list block in all three size regimes, identity/rules section shortening on `sends_native_tool_specs`, the verify-Test-Path rule
  - `sessions.rs` — title generation (Unicode, truncation, empty), path-traversal rejection, create/load roundtrip, ordering by `last_active_at` desc, atomic write (tmp + rename), skip-corrupt-files, delete idempotency
  - `router.rs` — every prefix permutation including `anthropic:anthropic/...` and bare `anthropic/...`
  - `skills.rs` — every safety rule, case-insensitive matches, the curl-without-pipe-is-safe regression, wget-pipe-to-bash detection
  - `config.rs` — legacy `openai`/`gemini` field drop, default seed
  - `tools/*.rs` — registry lookup, `ask_user_question` validation, PowerShell exec + failure exit codes
  - `agent.rs` — tool-proposal extraction, AgentStepResult serialization shape, loop-verdict escalation

- Frontend: **19 vitest tests** (`npm test`).
  - `agent.test.ts` — `isTaskComplete` patterns
  - `useError.test.ts` — show/clear/auto-dismiss timing
  - `QuestionApproval.test.tsx` — radio + multi-select + custom-answer paths
  - `ChatMessage.test.tsx` — synthetic vs real-user rendering distinction (the "shuttle internal" badge)

Run both via `cargo test --lib && npm test` from project root.

---

## Build & ship

```powershell
# Dev (hot-reload frontend, Rust recompiles on save)
npm run tauri dev

# Release MSIX / NSIS installer
npm run tauri build
```

Output lands in `src-tauri/target/release/bundle/`.

---

## Known limitations / pending work

- **Small Ollama models** still hallucinate "Task Complete" without verifying
  output files. The system prompt forces `Test-Path` checks now (rule 5), but
  weak models will sometimes still claim success. Lean on Claude/GPT/Gemini
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

---

## License

MIT. Personal-use desktop assistant — third-party provider terms apply (don't
ship shuttle-io as a SaaS product without dealing with Anthropic / OpenAI /
Google ToS yourself).
