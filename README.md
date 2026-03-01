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
| `features/agent.rs` | Agent loop, system prompt construction, PowerShell execution, tool call parsing |
| `features/explorer.rs` | Windows Explorer COM integration — **only OS-specific module** |
| `llm/clients/` | LLM providers: Ollama, OpenAI, Gemini, Antigravity. Routed by model prefix (`"openai:*"` / `"gemini:*"` / `"antigravity"` / else = Ollama) |
| `utils/config.rs` | Config persistence in `AppData/Roaming/shuttle-io/` |
| `auth/antigravity.rs` | OAuth2 flow for Antigravity (Google internal) |

### Agent Flow

1. User prompt + Explorer context (path, selected files) → system prompt constructed in `agent.rs`
2. LLM responds with text or a `run_powershell` tool call
3. Tool calls become command proposals — user approves/edits/rejects before execution
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
| `src-tauri/src/lib.rs` | Command registration, window/tray/hotkey setup |
| `src-tauri/src/features/agent.rs` | Agent orchestration & system prompt |
| `src-tauri/src/features/explorer.rs` | Windows Explorer COM integration |
| `src-tauri/src/llm/clients/` | LLM provider clients (ollama, openai, gemini, antigravity) |
| `src-tauri/src/utils/config.rs` | Config management |
| `src-tauri/src/auth/antigravity.rs` | OAuth2 flow for Antigravity |

### Tauri IPC Commands

Defined in `src-tauri/src/lib.rs`:

**Explorer:**
- `get_explorer_status` → `{ path, selected_files }`
- `get_explorer_debug` → debug info for all Explorer windows

**Agent:**
- `init_agent_conversation(context_path, selected_files, user_prompt)` → `Vec<Message>`
- `run_agent_step(model, history)` → `AgentStepResult` (Text or CommandProposal)
- `execute_powershell(command, cwd)` → output string (only after user approval)

**Window Management:**
- `spotlight_submit(prompt, model)` → hides spotlight, shows main, emits `spotlight-submitted` event

**Models & Config:**
- `get_ollama_models` / `get_ollama_url` / `set_ollama_url`
- `get_api_keys` / `set_openai_key` / `set_gemini_key`
- `get_hotkey` / `set_hotkey`
- `login_antigravity` / `get_antigravity_status`

**Utility:**
- `write_file_list(files)` → temp file path (for large selections >20 files)

### Events (Rust → JS)

- `spotlight-submitted` → `{ prompt, model }` — main window listens for this to start a conversation from spotlight input

### Configuration

All config in `AppData/Roaming/shuttle-io/`:
- `shuttle_config.json` — Ollama URL, hotkey (default `Ctrl+Alt+A`)
- `api_keys.json` — OpenAI/Gemini API keys
- `antigravity_credentials.json` — OAuth tokens

### Cross-Platform Notes

Window positioning uses Tauri's cross-platform `cursor_position()` and `available_monitors()` APIs. `features/explorer.rs` is the only Windows-specific module (COM via `windows` crate). Porting to another OS means replacing that single module.

---

## Getting Started

**Prerequisites**: Node.js v20+, Rust (stable), Ollama (optional)

```bash
npm install
npm run tauri dev     # dev server on port 1420
npm run tauri build   # production build in src-tauri/target/release/
```

---

## Troubleshooting

- **Explorer Sync Fails**: Run as Administrator for Admin-privileged Explorer windows.
- **Ollama Connection**: Ensure `ollama serve` is running, check URL in settings.
- **Port 1420 in use**: Kill the existing Vite process before dev.
- **Hotkey conflict**: Edit `hotkey` in `shuttle_config.json`.
