# Shuttle.io

Context-aware AI assistant that integrates with Windows Explorer. Select files in Explorer, describe what you want, and an LLM proposes PowerShell commands for your approval.

Built with [Tauri v2](https://v2.tauri.app/), React 19, TypeScript, and Rust.

---

## Architecture

### Two-Window Design

Both windows load the same React app (`src/App.tsx`). React routes by `getCurrentWindow().label`:

- **Spotlight** (`spotlight`): Small transparent borderless input card. Always-on-top, hidden by default. Summoned by global hotkey (`Ctrl+Alt+A`), centered on the cursor's monitor.
- **Main** (`main`): Full conversation UI with chat, command proposals, settings. Opened when spotlight submits a prompt, or from the system tray.

```
Hotkey → Spotlight appears → User types + Enter → Spotlight hides, Main opens with conversation
Tray click → Main opens
Close Main → Hides to tray
Blur/Esc → Spotlight hides
```

### Backend (`src-tauri/src/`)

| Module | Purpose |
|---|---|
| `lib.rs` | Command registration, window/tray/hotkey setup, `center_on_cursor_monitor()` |
| `features/agent.rs` | Agent loop (blocking + streaming), nudge logic, PowerShell execution |
| `features/prompts.rs` | System prompt template with skills injection |
| `features/skills.rs` | SKILL.md parsing, binary detection, prompt section generation |
| `features/explorer.rs` | Windows Explorer COM integration — **only OS-specific module** |
| `llm/mod.rs` | Shared types: `Message`, `ToolCall`, `StreamChunk` |
| `llm/router.rs` | Model routing (`route_chat` + `route_chat_stream`) by prefix |
| `llm/clients/` | LLM providers: Ollama, OpenAI, Gemini, Antigravity, Anthropic |
| `utils/config.rs` | Config persistence in `AppData/Roaming/shuttle-io/` |
| `auth/antigravity.rs` | OAuth2 PKCE flow for Antigravity (Google internal) |
| `auth/cli_credentials.rs` | Claude Code OAuth (read, refresh, expiry) + Codex CLI credentials |

### LLM Providers

Models are routed by prefix in `llm/router.rs`:

| Prefix | Provider | Credentials | Streaming |
|---|---|---|---|
| `claude:*` | Anthropic API | Claude Code OAuth (`~/.claude/.credentials.json`) — auto-refreshes | SSE |
| `codex:*` | OpenAI API | Codex CLI (`~/.codex/auth.json` or `OPENAI_API_KEY`) | SSE |
| `antigravity` | Google Antigravity | OAuth2 PKCE login | SSE |
| `openai:*` | OpenAI API | Manual API key in settings | SSE |
| `gemini:*` | Google Gemini | Manual API key in settings | SSE |
| *(default)* | Ollama (local) | None (local server) | NDJSON |

All providers stream responses in real-time via `tauri::ipc::Channel<StreamChunk>`.

### Skills System

Markdown files with YAML frontmatter that inject domain knowledge into the system prompt when tools are available on the system.

```yaml
---
name: ffmpeg
description: Video and audio processing with FFmpeg
requires:
  bins: [ffmpeg]          # ALL must be on PATH
  anyBins: [magick, convert]  # At least ONE on PATH (optional)
---
# Markdown body injected into system prompt
```

**Locations:**
- `src-tauri/skills/` — bundled with the app (ffmpeg, oiiotool, imagemagick)
- `AppData/Roaming/shuttle-io/skills/` — user-defined (overrides bundled by name)

Skills are checked at conversation init. Available skills appear in the settings panel.

### Agent Flow

1. User prompt + Explorer context (path, selected files) → system prompt constructed with skills
2. LLM streams response in real-time via `StreamChunk::TextDelta`
3. Tool calls or markdown code blocks become command proposals — user approves/edits/rejects
4. PowerShell output fed back to LLM → loop continues
5. Auto-execute mode runs proposals automatically (capped at 10 steps)
6. Nudge loop retries up to 3x if LLM doesn't use tool calls. Fallback extracts commands from markdown code blocks.
7. Output truncated to 2000 chars to prevent token overflow.

---

## Guide for Agents & Developers

### Key Files

| File | Purpose |
|---|---|
| `src/App.tsx` | All frontend — `SpotlightApp` (lightweight input) + `MainApp` (full UI) |
| `src/types/index.ts` | Shared TypeScript types |
| `src/constants.ts` | All magic values (timings, limits, command names, events) |
| `src/utils/tauri.ts` | Typed Tauri invoke wrappers (no raw `invoke()` in components) |
| `src/hooks/` | Custom hooks: `useModels`, `useExplorer`, `useError` |
| `src/components/` | UI components: ChatMessage, CommandApproval, ChatInput, ModelSelector, TitleBar, ErrorToast |
| `src-tauri/src/lib.rs` | Command registration, window/tray/hotkey setup |
| `src-tauri/src/features/agent.rs` | Agent orchestration (blocking + streaming) |
| `src-tauri/src/features/prompts.rs` | System prompt template with skills injection |
| `src-tauri/src/features/skills.rs` | SKILL.md loading, binary checks, prompt generation |
| `src-tauri/src/features/explorer.rs` | Windows Explorer COM integration |
| `src-tauri/src/llm/router.rs` | Model routing (`route_chat` + `route_chat_stream`) |
| `src-tauri/src/llm/clients/` | LLM clients: ollama, openai, gemini, antigravity, anthropic |
| `src-tauri/src/auth/cli_credentials.rs` | Claude Code + Codex CLI credential reading |
| `src-tauri/src/auth/antigravity.rs` | OAuth2 PKCE flow for Antigravity |
| `src-tauri/src/utils/config.rs` | Config management |
| `src-tauri/skills/*.md` | Bundled skill definitions (ffmpeg, oiiotool, imagemagick) |

### Tauri IPC Commands

Defined in `src-tauri/src/lib.rs`:

**Explorer:**
- `get_explorer_status` → `{ path, selected_files }`
- `get_explorer_debug` → debug info for all Explorer windows

**Agent:**
- `init_agent_conversation(context_path, selected_files, user_prompt)` → `Vec<Message>` (loads skills, builds system prompt)
- `run_agent_step(model, history)` → `AgentStepResult` (blocking, no streaming)
- `run_agent_step_stream(model, history, on_chunk)` → `AgentStepResult` (streams `StreamChunk` via Tauri Channel)
- `execute_powershell(command, cwd)` → output string (only after user approval)

**Window Management:**
- `spotlight_submit(prompt, model)` → hides spotlight, shows main, emits `spotlight-submitted` event

**Models & Config:**
- `get_ollama_models` / `get_ollama_url` / `set_ollama_url`
- `get_api_keys` / `set_openai_key` / `set_gemini_key`
- `get_hotkey` / `set_hotkey`
- `login_antigravity` / `get_antigravity_status`
- `get_cli_credentials_status` → `{ claude_code: bool, codex: bool }`
- `list_skills` → `Vec<Skill>` (all loaded skills with availability status)

**Utility:**
- `write_file_list(files)` → temp file path (for large selections >20 files)

### Events (Rust → JS)

- `spotlight-submitted` → `{ prompt, model }` — main window listens for this to start a conversation from spotlight input

### Configuration

All config in `AppData/Roaming/shuttle-io/`:
- `shuttle_config.json` — Ollama URL, hotkey (default `Ctrl+Alt+A`)
- `api_keys.json` — OpenAI/Gemini API keys
- `antigravity_credentials.json` — OAuth tokens
- `skills/` — User-defined skill files (override bundled by name)

**Auto-detected credentials (no config needed):**
- Claude Code: `~/.claude/.credentials.json` (run `claude` to log in) — OAuth tokens auto-refresh
- Codex CLI: `~/.codex/auth.json` or `OPENAI_API_KEY` env var

### Cross-Platform Notes

Window positioning uses Tauri's cross-platform `cursor_position()` and `available_monitors()` APIs. `features/explorer.rs` is the only Windows-specific module (COM via `windows` crate). Porting to another OS means replacing that single module.

### Adding Custom Skills

Create a `.md` file in `AppData/Roaming/shuttle-io/skills/`:

```yaml
---
name: my-tool
description: What this tool does
requires:
  bins: [my-tool]
---
You have **my-tool** available. Use it for:
- **Operation**: `my-tool --flag input output`
```

The skill body is injected into the system prompt when all required binaries are found on PATH. Use `bins` for ALL-required and `anyBins` for ANY-of-these-required.

---

## Getting Started

**Prerequisites**: Node.js v20+, Rust (stable), Ollama (optional)

```bash
npm install
npm run tauri dev     # dev server on port 1420
npm run tauri build   # production build in src-tauri/target/release/
```

**Optional LLM providers** (auto-detected if installed):
- [Ollama](https://ollama.ai/) — local models, no API key needed
- [Claude Code](https://claude.ai/claude-code) — log in with `claude`, models appear as `claude:*`
- [Codex CLI](https://github.com/openai/codex) — set `OPENAI_API_KEY` or install CLI, models appear as `codex:*`
- OpenAI / Gemini API keys — enter in Settings panel

---

## OAuth Authentication

Shuttle uses OAuth flows for two providers: **Anthropic** (Claude Code credentials) and **Antigravity** (Google internal). Both auto-refresh tokens when expired. The OAuth implementations are based on research from **[pi-ai](https://github.com/badlogic/pi-mono)** (`@mariozechner/pi-ai` on npm), used by [OpenClaw](https://github.com/openclaw/openclaw).

### Anthropic (Claude Code Credentials)

Shuttle borrows Claude Code's OAuth tokens to call the Anthropic API. This is a subscription-based auth path (Claude Pro/Max), not API key billing.

**Flow:**
1. User logs into Claude Code CLI (`claude`) — this stores OAuth tokens in `~/.claude/.credentials.json`
2. Shuttle reads the `claudeAiOauth` object: `accessToken` (prefix `sk-ant-oat-*`), `refreshToken`, `expiresAt` (milliseconds)
3. On each request, Shuttle checks expiry and auto-refreshes via `https://console.anthropic.com/v1/oauth/token` if needed
4. OAuth tokens require different headers than API keys:

| Header | Value | Why |
|---|---|---|
| `Authorization` | `Bearer <token>` | OAuth uses Bearer, not `x-api-key` |
| `anthropic-beta` | `claude-code-20250219,oauth-2025-04-20` | Required for OAuth tokens |
| `user-agent` | `claude-cli/2.1.62` | Claude Code identity |
| `x-app` | `cli` | Claude Code identity |

### Maintaining Auth When It Breaks

The OAuth protocol details (headers, endpoints, client ID) can change. Our implementation is based on **[pi-ai](https://github.com/badlogic/pi-mono)** (`@mariozechner/pi-ai` on npm), which is used by [OpenClaw](https://github.com/openclaw/openclaw) and tracks Anthropic's OAuth changes closely.

**When Claude auth breaks, check these files in pi-mono:**

| What to check | File in `badlogic/pi-mono` |
|---|---|
| Required HTTP headers, beta flags | `packages/ai/src/providers/anthropic.ts` → `createClient()` → `isOAuthToken` branch |
| Token refresh endpoint, client ID | `packages/ai/src/utils/oauth/anthropic.ts` → `TOKEN_URL`, `CLIENT_ID` |
| Token format / field names | `packages/ai/src/utils/oauth/anthropic.ts` → `loginAnthropic()` return value |

**Current values (update if they change):**
- Token URL: `https://console.anthropic.com/v1/oauth/token`
- Client ID: `9d1c250a-e61b-44d9-88ed-5944d1962f5e`
- OAuth token prefix: `sk-ant-oat`
- Beta header: `claude-code-20250219,oauth-2025-04-20`
- Claude CLI version in user-agent: `2.1.62`

**Quick test without the app:**
```bash
curl https://api.anthropic.com/v1/messages \
  -H "Authorization: Bearer <token-from-credentials-file>" \
  -H "anthropic-version: 2023-06-01" \
  -H "anthropic-beta: claude-code-20250219,oauth-2025-04-20" \
  -H "user-agent: claude-cli/2.1.62" \
  -H "x-app: cli" \
  -H "content-type: application/json" \
  -d '{"model":"claude-sonnet-4-20250514","max_tokens":100,"messages":[{"role":"user","content":"hi"}]}'
```

### Antigravity (Google Internal)

Shuttle has a built-in OAuth2 PKCE login flow for Antigravity (Google's internal Gemini endpoint). Unlike Anthropic, this doesn't borrow credentials from another CLI — Shuttle handles the entire login.

**Flow:**
1. User clicks "Login" in Settings → browser opens Google OAuth consent
2. Local callback server on `localhost:51121` captures the auth code
3. Code exchanged for tokens via `https://oauth2.googleapis.com/token`
4. Tokens stored in `AppData/Roaming/shuttle-io/antigravity_credentials.json`
5. Auto-refreshes on expiry (5-minute buffer) before each API call
6. API calls go to `https://daily-cloudcode-pa.sandbox.googleapis.com/` with `Authorization: Bearer` + Claude Code-style stealth headers

**Key files:**
- `src-tauri/src/auth/antigravity.rs` — Full OAuth2 PKCE login, token refresh, credential storage
- `src-tauri/src/llm/clients/antigravity.rs` — Antigravity API client (SSE streaming, tool calls)

**Current values (update if they change):**
- Client ID: `1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com`
- Token URL: `https://oauth2.googleapis.com/token`
- API URL: `https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:streamGenerateContent?alt=sse`
- Callback port: `51121`

**When Antigravity auth breaks, check pi-mono:**

| What to check | File in `badlogic/pi-mono` |
|---|---|
| Client ID, scopes, endpoints | `packages/ai/src/utils/oauth/google-antigravity.ts` |
| API request format, headers | Check OpenClaw's `src/agents/` for Antigravity-specific handling |

**503 "MODEL_CAPACITY_EXHAUSTED":** Shuttle auto-retries up to 3 times with increasing backoff (10s, 20s, 30s). The user sees a `[Capacity unavailable, retrying...]` message while waiting.

---

## Troubleshooting

- **Explorer Sync Fails**: Run as Administrator for Admin-privileged Explorer windows.
- **Ollama Connection**: Ensure `ollama serve` is running, check URL in settings.
- **Port 1420 in use**: Kill the existing Vite process before dev.
- **Hotkey conflict**: Edit `hotkey` in `shuttle_config.json`.
- **Claude Code "expired"**: Shuttle auto-refreshes tokens. If it still fails, run `claude` in a terminal to re-authenticate, then check the maintenance section above.
- **Claude 401/403 errors**: Headers or beta flags may have changed — check pi-ai reference above.
- **Skills not showing**: Check that the required binary is on PATH (`where.exe <binary>`).
