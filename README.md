# Shuttle.io 🚀

**Shuttle.io** is a context-aware AI assistant that integrates deep into your Windows workflow. Its primary goal is to **facilitate running complex commands on your files**, with a strong focus on processing image files, video files and other assets directly from your active Windows Explorer window.

By leveraging local LLMs (Ollama) or cloud providers (OpenAI, Gemini), it understands your intent and orchestrates file operations that would otherwise require complex manual scripting.

This project is built with [Tauri v2](https://v2.tauri.app/), [React](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), and [Vite](https://vitejs.dev/).

---

## 🏗️ Architecture Overview

The application is split into two main parts: the **Rust Backend** (Tauri process) and the **React Frontend** (Webview).

### 🖥️ Frontend (`src/`)
- **Technology**: React 19, TypeScript, TailwindCSS v4.
- **Entry Point**: `src/main.tsx` -> `src/App.tsx`.
- **Key Components**:
    - **`App.tsx`**: Manages the entire application state, including chat history, explorer sync status, and settings.
    - **UI**: Uses `lucide-react` for icons and a custom dark theme.
- **Communication**: Uses `invoke` from `@tauri-apps/api/core` to call Rust backend commands.

### 🦀 Backend (`src-tauri/src/`)
- **Technology**: Rust, Tauri v2.
- **Entry Point**: `src-tauri/src/lib.rs` (exposes commands).
- **Key Modules**:
    - **`lib.rs`**: Registers all Tauri commands and initializes the app.
    - **`features/explorer.rs`**: **Core Feature**. Uses Windows COM APIs (`IShellWindows`) to find the active Explorer window, its current path, and any selected files.
    - **`features/agent.rs`**: Handles the agent logic, prompt construction, and PowerShell command execution.
    - **`llm/`**: Handles communication with LLM providers (Ollama, OpenAI, Gemini).
    - **`utils/config.rs`**: Manages persistent configuration (stored in `AppData/shuttle-io/`).

---

## 🤖 Guide for Agents & Developers

If you are an AI Agent or Developer adding features, here is what you need to know:

### 1. Where to seek "Brains" 🧠
- **Explorer Logic**: If you need to fix how files are detected or paths are read, look at `src-tauri/src/features/explorer.rs`. It uses `windows-rs`.
- **LLM Client**: If you need to add a new provider or fix prompt issues, check `src-tauri/src/llm/` and `src-tauri/src/llm/clients/`.
- **Agent Behavior**: The loop for "Think -> Propose Command -> Execute" is handled in `src-tauri/src/features/agent.rs` and orchestrated in `src/App.tsx`.

### 2. Key Commands (Rust -> JS)
Defined in `src-tauri/src/lib.rs`:
- `get_explorer_status`: Returns `{ path: String, selected_files: Vec<String> }`.
- `init_agent_conversation`: Starts a new chat session with context.
- `run_agent_step`: Sends the chat history to the LLM and processes the response.
- `execute_powershell`: Runs a command (only after user approval).
- `get_ollama_models` / `get_ollama_url`: Manage Ollama connection.

### 3. Configuration ⚙️
- **Ollama URL**: Stored in `shuttle_config.json`.
- **API Keys**: Stored in `api_keys.json` (OpenAI, Gemini).
- **Location**: Use `dirs::config_dir()` + `/shuttle-io/`.

---

## 🚀 Getting Started

### Prerequisites
- **Node.js** (v20+)
- **Rust** (Latest Stable)
- **Ollama** (optional, for local LLMs)

### Installation

1.  **Clone the repository**:
    ```bash
    git clone <repository-url>
    cd shuttle-io
    ```

2.  **Install Frontend Dependencies**:
    ```bash
    npm install
    ```
    *(Note: Using `npm` based on lockfile)*

3.  **Run Development Server**:
    ```bash
    npm run tauri dev
    ```
    This will start the Vite server and launch the Tauri application window.

### Building for Production
```bash
npm run tauri build
```
The executable will be located in `src-tauri/target/release/`.

---

## ✨ Features

- **Context Sync**: Click the "Sync" button (or folder icon) to instantly target the current path and selected files in your active Explorer window for processing.
- **Command Execution**: The Agent can propose PowerShell commands. You see the command first, then click to approve/run it.
- **Multi-Provider**: Switch between Ollama (local), OpenAI, and Gemini.
- **Auto-Scroll & Markdown**: Rich chat interface with code highlighting.

---

## 🛠️ Troubleshooting

- **Explorer Sync Fails**: Run the app as Administrator if you are trying to access Admin-privileged Explorer windows.
- **Ollama Connection**: Ensure Ollama is running (`ollama serve`) and the URL in settings is correct (default `http://localhost:11434`).
