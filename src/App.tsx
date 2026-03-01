import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ReactMarkdown from "react-markdown";
import {
  Terminal,
  Info,
  File,
  Play,
  X,
  Bot,
  User,
  Loader2,
  ChevronDown,
  ChevronUp,
  StopCircle,
  FolderSync,
  RefreshCw,
  Settings,
  Minus,
  Square,
  ArrowUp
} from "lucide-react";
import "./App.css";

// Types matching Rust
interface ExplorerState {
  path: string;
  selected_files: string[];
}

interface Message {
  role: string;
  content: string;
  tool_calls?: any;
}

interface AgentStepResult {
  response: {
    type: "Text" | "CommandProposal";
    content: string;
  };
  updated_history: Message[];
}

const WINDOW_LABEL = getCurrentWindow().label;

function App() {
  if (WINDOW_LABEL === "spotlight") return <SpotlightApp />;
  return <MainApp />;
}

// ═══════════════════════════════════════════════════════════════
// SpotlightApp — lightweight input window
// ═══════════════════════════════════════════════════════════════

function SpotlightApp() {
  const [explorerState, setExplorerState] = useState<ExplorerState | null>(null);
  const [agentPrompt, setAgentPrompt] = useState("");
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null);
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const shownAt = useRef(0);

  // Fetch models
  const fetchModels = async () => {
    try {
      const models = await invoke<string[]>("get_ollama_models");
      setAvailableModels(models);
      setOllamaConnected(true);
      if (models.length > 0 && !modelName) {
        setModelName(models[0]);
      }
    } catch {
      setOllamaConnected(false);
      setAvailableModels([]);
    }
  };

  // Fetch explorer state
  const fetchExplorer = async () => {
    try {
      const state = await invoke<ExplorerState>("get_explorer_status");
      setExplorerState(state);
    } catch {
      setExplorerState(null);
    }
  };

  // On mount: fetch models + explorer
  useEffect(() => {
    fetchModels();
    fetchExplorer();
  }, []);

  // On window focus: re-fetch explorer state + focus input
  useEffect(() => {
    const unlisten = getCurrentWindow().onFocusChanged(async ({ payload: focused }) => {
      if (focused) {
        shownAt.current = Date.now();
        fetchExplorer();
        setTimeout(() => inputRef.current?.focus(), 50);
      } else {
        // Grace period: ignore blur within 300ms of showing
        if (Date.now() - shownAt.current > 300) {
          getCurrentWindow().hide();
        }
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Escape to hide
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Submit: hand off to main window via Rust
  async function submit() {
    if (!agentPrompt.trim()) return;
    try {
      await invoke("spotlight_submit", { prompt: agentPrompt, model: modelName });
      setAgentPrompt("");
    } catch (e) {
      setError(String(e));
    }
  }

  const spotlightPath = explorerState?.path || "";
  const spotlightFileCount = explorerState?.selected_files.length || 0;
  const spotlightFolder = spotlightPath.split("\\").filter(Boolean).pop() || spotlightPath;

  return (
    <main className="flex flex-col h-screen text-gray-100 font-sans overflow-hidden">
      <div className="flex-1 flex items-center justify-center drag-region">
        <div className="w-[560px] bg-gray-900/95 backdrop-blur-xl border border-gray-700/50 rounded-2xl shadow-2xl shadow-black/50 p-6 flex flex-col items-center gap-4 no-drag">
          <Bot size={36} className="text-indigo-500 opacity-80" />
          <input
            ref={inputRef}
            className="w-full bg-black/40 border border-gray-700/60 text-xl text-gray-200 placeholder-gray-600 rounded-xl px-5 py-3 focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/30 font-medium"
            placeholder="What would you like to do?"
            value={agentPrompt}
            onChange={(e) => setAgentPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
            autoFocus
          />
          <div className="flex flex-col items-center gap-1 text-xs text-gray-500">
            {explorerState && (
              <div className="flex items-center gap-1.5">
                <FolderSync size={12} className="text-gray-600" />
                <span>{spotlightFileCount} file{spotlightFileCount !== 1 ? "s" : ""}</span>
                <span className="text-gray-700">·</span>
                <span className="truncate max-w-48">{spotlightFolder}</span>
              </div>
            )}
            <div className="flex items-center gap-2 text-gray-600">
              <select
                className="bg-transparent text-xs text-gray-600 focus:outline-none cursor-pointer"
                value={modelName}
                onChange={(e) => setModelName(e.target.value)}
                disabled={availableModels.length === 0}
              >
                {ollamaConnected === null ? (
                  <option value="">Checking...</option>
                ) : ollamaConnected === false ? (
                  <option value="">No Ollama</option>
                ) : availableModels.length === 0 ? (
                  <option value="">No models</option>
                ) : (
                  availableModels.map(m => <option key={m} value={m}>{m}</option>)
                )}
              </select>
              <span className="text-gray-700">·</span>
              <span>Esc to dismiss</span>
            </div>
          </div>
        </div>
      </div>

      {error && (
        <div className="absolute top-4 right-4 bg-red-900/95 text-red-200 text-xs px-3 py-2 rounded-lg border border-red-700 shadow-lg z-50 flex items-center gap-2 max-w-md">
          <span className="flex-1">{error}</span>
          <button onClick={() => setError("")} className="p-1 hover:bg-red-800 rounded transition shrink-0">
            <X size={12} />
          </button>
        </div>
      )}
    </main>
  );
}

// ═══════════════════════════════════════════════════════════════
// MainApp — full conversation UI
// ═══════════════════════════════════════════════════════════════

function MainApp() {
  const [explorerState, setExplorerState] = useState<ExplorerState | null>(null);
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState(false);

  // Agent State
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null);
  const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [tempOllamaUrl, setTempOllamaUrl] = useState("");
  const [agentPrompt, setAgentPrompt] = useState("");
  const [antigravityEmail, setAntigravityEmail] = useState<string | null>(null);

  // Provider State
  const [openaiKey, setOpenaiKey] = useState("");
  const [geminiKey, setGeminiKey] = useState("");

  // Conversation State
  const [chatHistory, setChatHistory] = useState<Message[]>([]);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [autoExecute, setAutoExecute] = useState(false);
  const stopRequested = useRef(false);
  const autoStepCount = useRef(0);

  // Input ref for hotkey focus
  const inputRef = useRef<HTMLInputElement>(null);

  // UI State
  const [isContextOpen, setIsContextOpen] = useState(false);
  const [feedbackText, setFeedbackText] = useState("");
  const [isFeedbackOpen, setIsFeedbackOpen] = useState(false);
  const [expandedResponses, setExpandedResponses] = useState<Record<number, boolean>>({});
  const chatEndRef = useRef<HTMLDivElement>(null);

  // Context Sync Tracking
  const lastSyncedContext = useRef<{ path: string; files: string[] } | null>(null);
  const [syncPending, setSyncPending] = useState(false);
  const [showSyncTooltip, setShowSyncTooltip] = useState(false);

  // Set opaque background for main window
  useEffect(() => {
    document.documentElement.style.background = '#030712';
    document.body.style.background = '#030712';
  }, []);

  // Fetch saved Ollama URL on mount
  useEffect(() => {
    const loadUrl = async () => {
      try {
        const url = await invoke<string>("get_ollama_url");
        setOllamaUrl(url);
        setTempOllamaUrl(url);
      } catch (err) {
        console.error("Failed to load Ollama URL:", err);
      }
    };
    loadUrl();
    invoke<string | null>("get_antigravity_status").then(setAntigravityEmail).catch(console.error);
    invoke<{ openai: string | null; gemini: string | null }>("get_api_keys").then(keys => {
      setOpenaiKey(keys.openai || "");
      setGeminiKey(keys.gemini || "");
    }).catch(console.error);
  }, []);

  // Listen for spotlight submission
  useEffect(() => {
    const unlisten = listen<{ prompt: string; model: string }>("spotlight-submitted", async (event) => {
      const { prompt, model } = event.payload;
      setModelName(model);
      setIsProcessing(true);
      try {
        const state = await invoke<ExplorerState>("get_explorer_status");
        setExplorerState(state);
        const history = await invoke<Message[]>("init_agent_conversation", {
          contextPath: state.path,
          selectedFiles: state.selected_files,
          userPrompt: prompt
        });
        lastSyncedContext.current = { path: state.path, files: [...state.selected_files] };
        setSyncPending(false);
        setChatHistory(history);

        await runAgentStep(history, autoExecute, model);
      } catch (e) {
        setError(String(e));
        setIsProcessing(false);
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Fetch models from Ollama
  const fetchOllamaModels = async () => {
    try {
      const models = await invoke<string[]>("get_ollama_models");
      setAvailableModels(models);
      setOllamaConnected(true);
      if (models.length > 0 && !modelName) {
        setModelName(models[0]);
      }
    } catch (err) {
      console.error("Failed to fetch models:", err);
      setOllamaConnected(false);
      setAvailableModels([]);
    }
  };

  // Initial fetch and periodic polling when not connected or no models
  useEffect(() => {
    fetchOllamaModels();
  }, [ollamaUrl]);

  useEffect(() => {
    if (ollamaConnected === false || (ollamaConnected === true && availableModels.length === 0)) {
      const interval = setInterval(fetchOllamaModels, 5000);
      return () => clearInterval(interval);
    }
  }, [ollamaConnected, availableModels.length]);

  // Auto-scroll to bottom
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chatHistory, pendingCommand, isProcessing, isExecuting]);

  async function getExplorerStatus() {
    setLoading(true);
    try {
      const debugInfo = await invoke<any>("get_explorer_debug");
      console.log("Explorer Debug Info:", debugInfo);

      const state = await invoke<ExplorerState>("get_explorer_status");
      setExplorerState(state);
      setError("");

      if (chatHistory.length > 0) {
        const currentFiles = lastSyncedContext.current?.files || [];
        const filesChanged =
          state.selected_files.length !== currentFiles.length ||
          state.selected_files.some((f, i) => f !== currentFiles[i]);

        if (filesChanged) {
          setSyncPending(true);
        }
      }
    } catch (e) {
      setError(String(e));
      setExplorerState(null);
    } finally {
      setLoading(false);
    }
  }

  // Step 1: Initialize Conversation
  async function startAgent() {
    if (!explorerState || !agentPrompt.trim()) return;

    const promptToSend = agentPrompt;
    setAgentPrompt("");
    setIsProcessing(true);
    autoStepCount.current = 0;
    setPendingCommand(null);

    try {
      let history = chatHistory;

      if (history.length === 0) {
        history = await invoke<Message[]>("init_agent_conversation", {
          contextPath: explorerState.path,
          selectedFiles: explorerState.selected_files,
          userPrompt: promptToSend
        });
        lastSyncedContext.current = {
          path: explorerState.path,
          files: [...explorerState.selected_files]
        };
        setSyncPending(false);
        setChatHistory(history);

        await runAgentStep(history);
      } else {
        let newHistory = [...history];

        if (syncPending && explorerState) {
          let contextUpdate: string;

          const FILE_LIST_THRESHOLD = 20;
          if (explorerState.selected_files.length > FILE_LIST_THRESHOLD) {
            const fileListPath = await invoke<string>("write_file_list", {
              files: explorerState.selected_files
            });
            contextUpdate = `[CONTEXT UPDATE - Files have changed]\n\nWorking directory: ${explorerState.path}\nSelected files: ${explorerState.selected_files.length} files (too many to list inline)\n\nThe complete file list has been written to: ${fileListPath}\n\nTo read the file list in PowerShell, use:\n$files = Get-Content '${fileListPath}'\n\nPlease use these updated files for any subsequent operations.`;
          } else {
            const filesList = explorerState.selected_files.length > 0
              ? explorerState.selected_files.map((f, i) => `${i + 1}. ${f}`).join('\n')
              : 'No specific files selected';
            contextUpdate = `[CONTEXT UPDATE - Files have changed]\n\nWorking directory: ${explorerState.path}\nSelected files (${explorerState.selected_files.length} total):\n${filesList}\n\nPlease use these updated files for any subsequent operations.`;
          }

          newHistory.push({ role: "user", content: contextUpdate });

          lastSyncedContext.current = {
            path: explorerState.path,
            files: [...explorerState.selected_files]
          };
          setSyncPending(false);
        }

        newHistory.push({ role: "user", content: promptToSend });
        setChatHistory(newHistory);
        await runAgentStep(newHistory);
      }

    } catch (e) {
      console.error(`Error starting conversation: ${e}`);
      setError(String(e));
      setIsProcessing(false);
    }
  }

  // Step 2: Run Agent Step
  async function runAgentStep(history: Message[], shouldAutoExecute = autoExecute, modelOverride?: string) {
    try {
      const result = await invoke<AgentStepResult>("run_agent_step", {
        model: modelOverride || modelName,
        history: history
      });

      setChatHistory(result.updated_history);

      if (stopRequested.current) {
        stopRequested.current = false;
        return;
      }

      if (result.response.type === "CommandProposal") {
        const cmd = result.response.content;
        autoStepCount.current = 0;
        if (shouldAutoExecute) {
          setIsExecuting(true);
          setIsProcessing(false);
          try {
            const output = await invoke<string>("execute_powershell", {
              command: cmd,
              cwd: explorerState?.path || null
            });
            const newMsg: Message = { role: "tool", content: output };
            const newHistory = [...result.updated_history, newMsg];
            setChatHistory(newHistory);
            setIsExecuting(false);
            setIsProcessing(true);
            await runAgentStep(newHistory, true, modelOverride);
          } catch (e) {
            console.error(`Auto-execute failed: ${e}`);
            setIsExecuting(false);
            setAutoExecute(false);
            setPendingCommand(cmd);
          }
        } else {
          setPendingCommand(cmd);
        }
      } else {
        setPendingCommand(null);

        if (shouldAutoExecute) {
          const contentLower = result.response.content.toLowerCase();
          const isTaskDone =
            contentLower.includes("task complete") ||
            contentLower.includes("all done") ||
            contentLower.includes("successfully processed all") ||
            contentLower.includes("completed successfully") ||
            contentLower.includes("successfully") ||
            contentLower.includes("has been") ||
            contentLower.includes("have been") ||
            contentLower.includes("done") ||
            (contentLower.includes("complete") && contentLower.length < 200) ||
            (contentLower.includes("finished") && contentLower.length < 200);

          const MAX_AUTO_STEPS = 10;
          if (!isTaskDone && autoStepCount.current < MAX_AUTO_STEPS) {
            autoStepCount.current++;
            setTimeout(async () => {
              if (stopRequested.current) return;
              const newHistory = [...result.updated_history, { role: "user", content: "If there are remaining UNFINISHED steps, proceed to the next one. If all steps are already done, say 'Task Complete'. Do NOT repeat any step that has already been executed." }];
              setChatHistory(newHistory);
              setIsProcessing(true);
              await runAgentStep(newHistory, true, modelOverride);
            }, 500);
            return;
          }
        }

        setAutoExecute(false);
      }
    } catch (e) {
      console.error(`Error running agent step: ${e}`);
      setError(`LLM Error: ${String(e)}`);
    } finally {
      setIsProcessing(false);
    }
  }

  function stopAgent() {
    stopRequested.current = true;
    setAutoExecute(false);
    setIsProcessing(false);
    setIsExecuting(false);
    setPendingCommand(null);
  }

  // Step 3: Approve Command
  async function approveCommand(continueAuto = false) {
    if (!pendingCommand) return;

    setIsExecuting(true);
    try {
      const output = await invoke<string>("execute_powershell", {
        command: pendingCommand,
        cwd: explorerState?.path || null
      });

      const newMsg: Message = {
        role: "tool",
        content: output
      };
      const newHistory = [...chatHistory, newMsg];
      setChatHistory(newHistory);
      setPendingCommand(null);
      setIsExecuting(false);

      setIsProcessing(true);
      await runAgentStep(newHistory, continueAuto || autoExecute);

    } catch (e) {
      console.error(`Command Execution Failed: ${e}`);
      setIsExecuting(false);
      setIsProcessing(false);
    }
  }

  function toggleExpand(idx: number) {
    setExpandedResponses(prev => ({ ...prev, [idx]: !prev[idx] }));
  }

  const handleAntigravityLogin = async () => {
    try {
      const email = await invoke<string>("login_antigravity");
      setAntigravityEmail(email);
    } catch (e) {
      setError("Login Failed: " + String(e));
    }
  };

  const saveApiKeys = async () => {
    try {
      await invoke("set_openai_key", { key: openaiKey });
      await invoke("set_gemini_key", { key: geminiKey });
      setIsSettingsOpen(false);
      fetchOllamaModels();
    } catch (e) {
      setError("Failed to save keys: " + String(e));
    }
  };

  return (
    <main className="flex flex-col h-screen text-gray-100 font-sans overflow-hidden">
      <div className="flex flex-col h-full bg-gray-950">

      {/* Title Bar */}
      <div
        className="flex-none h-8 bg-gray-900 flex items-center px-3 select-none drag-region"
        onDoubleClick={() => getCurrentWindow().toggleMaximize()}
      >
        <div className="flex items-center gap-2 pointer-events-none">
          <Terminal size={14} className="text-indigo-500" />
          <span className="text-[11px] text-gray-500 font-medium tracking-wide">shuttle-io</span>
        </div>
        <div className="flex-1" />
        <div className="flex items-center no-drag -mr-2">
          <button
            onClick={() => getCurrentWindow().minimize()}
            className="w-10 h-8 flex items-center justify-center hover:bg-gray-800 text-gray-500 hover:text-gray-300 transition"
            title="Minimize"
          >
            <Minus size={14} />
          </button>
          <button
            onClick={() => getCurrentWindow().toggleMaximize()}
            className="w-10 h-8 flex items-center justify-center hover:bg-gray-800 text-gray-500 hover:text-gray-300 transition"
            title="Maximize"
          >
            <Square size={10} />
          </button>
          <button
            onClick={() => getCurrentWindow().hide()}
            className="w-10 h-8 flex items-center justify-center hover:bg-red-600 text-gray-500 hover:text-white transition"
            title="Close"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Toolbar */}
      <div className="flex-none h-10 bg-gray-900/50 border-b border-gray-800 flex items-center px-3 gap-2">
        {/* Left: Sync + Context */}
        <div className="flex items-center gap-1.5">
          <div
            className="relative"
            onMouseEnter={() => setShowSyncTooltip(true)}
            onMouseLeave={() => setShowSyncTooltip(false)}
          >
            <button
              onClick={getExplorerStatus}
              disabled={loading || isProcessing || isExecuting}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all disabled:opacity-50 ${syncPending
                ? 'bg-amber-500/20 text-amber-400 border border-amber-500/30 animate-pulse'
                : explorerState
                  ? 'bg-gray-800/80 text-gray-300 hover:bg-gray-700 border border-gray-700'
                  : 'bg-indigo-600 text-white hover:bg-indigo-500'
                }`}
              title={explorerState ? "Click to sync with Explorer" : "Sync with Explorer"}
            >
              {loading ? (
                <RefreshCw size={14} className="animate-spin" />
              ) : (
                <FolderSync size={14} />
              )}
              {explorerState ? (
                <span className="tabular-nums">{explorerState.selected_files.length}</span>
              ) : (
                <span>Sync</span>
              )}
            </button>

            {/* Tooltip showing file list on hover */}
            {showSyncTooltip && explorerState && explorerState.selected_files.length > 0 && (
              <div className="absolute top-full left-0 mt-2 z-50 bg-gray-900 border border-gray-700 rounded-lg shadow-xl p-3 min-w-64 max-w-sm animate-in fade-in slide-in-from-top-2 duration-150">
                <div className="text-[10px] text-gray-500 uppercase font-bold tracking-wider mb-2">Selected Files</div>
                <ul className="space-y-1 max-h-48 overflow-y-auto custom-scrollbar">
                  {explorerState.selected_files.map((file, i) => (
                    <li key={i} className="flex items-start gap-2 text-xs text-gray-400">
                      <File size={10} className="mt-0.5 shrink-0 text-gray-600" />
                      <span className="break-all">{file.split('\\').pop()}</span>
                    </li>
                  ))}
                </ul>
                {syncPending && (
                  <div className="mt-2 pt-2 border-t border-gray-800 text-[10px] text-amber-400">
                    Context will update with next message
                  </div>
                )}
              </div>
            )}
          </div>

          <button
            onClick={() => setIsContextOpen(!isContextOpen)}
            className={`p-1.5 rounded-md transition ${isContextOpen ? 'bg-gray-800 text-gray-200' : 'text-gray-500 hover:text-gray-300 hover:bg-gray-800/50'}`}
            title="Toggle Context Details"
          >
            <Info size={16} />
          </button>
        </div>

        <div className="flex-1" />

        {/* Right: New Chat + Settings */}
        <div className="flex items-center gap-1">
          {chatHistory.length > 0 && (
            <button
              onClick={() => {
                setChatHistory([]);
                setPendingCommand(null);
                setIsFeedbackOpen(false);
                setFeedbackText("");
                setExpandedResponses({});
                setIsProcessing(false);
                setIsExecuting(false);
              }}
              className="p-1.5 rounded-md text-gray-500 hover:text-gray-300 hover:bg-gray-800/50 transition"
              title="New Chat"
            >
              <X size={16} />
            </button>
          )}

          <div className="relative">
            <button
              onClick={() => {
                setTempOllamaUrl(ollamaUrl);
                setIsSettingsOpen(!isSettingsOpen);
              }}
              className={`p-1.5 rounded-md transition ${isSettingsOpen ? 'bg-gray-800 text-gray-200' : 'text-gray-500 hover:text-gray-300 hover:bg-gray-800/50'}`}
              title="Settings"
            >
              <Settings size={16} />
            </button>

            {/* Settings Popover */}
            {isSettingsOpen && (
              <div className="absolute top-full right-0 mt-2 z-50 bg-gray-900 border border-gray-700 rounded-lg shadow-xl p-4 min-w-72 animate-in fade-in slide-in-from-top-2 duration-150">
                <div className="text-[10px] text-gray-500 uppercase font-bold tracking-wider mb-3">Settings</div>
                <div className="space-y-3">
                  <div>
                    <label className="block text-xs text-gray-400 mb-1">Ollama Server URL</label>
                    <input
                      type="text"
                      value={tempOllamaUrl}
                      onChange={(e) => setTempOllamaUrl(e.target.value)}
                      className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-xs text-gray-200 focus:outline-none focus:border-indigo-500"
                      placeholder="http://localhost:11434"
                    />
                  </div>
                  <div className="pt-2 border-t border-gray-800">
                    <label className="block text-xs text-gray-400 mb-1">Antigravity Access</label>
                    {antigravityEmail ? (
                      <div className="flex items-center justify-between bg-emerald-900/20 border border-emerald-500/30 rounded px-2 py-1.5">
                        <span className="text-xs text-emerald-400 truncate max-w-[180px]" title={antigravityEmail}>{antigravityEmail}</span>
                        <Bot size={12} className="text-emerald-500" />
                      </div>
                    ) : (
                      <button
                        onClick={handleAntigravityLogin}
                        className="w-full py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-600 rounded text-xs transition flex items-center justify-center gap-2"
                      >
                        <Bot size={12} />
                        Login with Google
                      </button>
                    )}
                  </div>
                  <div className="pt-2 border-t border-gray-800 space-y-2">
                    <div>
                      <label className="block text-xs text-gray-400 mb-1">OpenAI API Key</label>
                      <input
                        type="password"
                        value={openaiKey}
                        onChange={(e) => setOpenaiKey(e.target.value)}
                        className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-xs text-gray-200 focus:outline-none focus:border-indigo-500"
                        placeholder="sk-..."
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-gray-400 mb-1">Gemini API Key</label>
                      <input
                        type="password"
                        value={geminiKey}
                        onChange={(e) => setGeminiKey(e.target.value)}
                        className="w-full bg-black/40 border border-gray-700 rounded px-2 py-1.5 text-xs text-gray-200 focus:outline-none focus:border-indigo-500"
                        placeholder="AIza..."
                      />
                    </div>
                  </div>
                  <div className="flex gap-2 pt-1 border-t border-gray-800 mt-2">
                    <button
                      onClick={async () => {
                        try {
                          await invoke("set_ollama_url", { url: tempOllamaUrl });
                          setOllamaUrl(tempOllamaUrl);
                          await saveApiKeys();
                          setOllamaConnected(null);
                        } catch (err) {
                          console.error("Failed to save URL:", err);
                        }
                      }}
                      className="flex-1 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-medium transition"
                    >
                      Save
                    </button>
                    <button
                      onClick={() => setIsSettingsOpen(false)}
                      className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded text-xs font-medium transition"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Context Drawer (collapsible) */}
      <div className={`flex-none bg-gray-900/90 backdrop-blur border-b border-gray-800 transition-all duration-300 ease-in-out overflow-hidden shadow-lg z-10 ${isContextOpen ? 'max-h-80' : 'max-h-0'}`}>
        {explorerState ? (
          <div className="p-4 grid grid-cols-1 gap-4 text-xs">
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className="text-gray-500 uppercase font-bold tracking-wider text-[10px]">Current Directory</span>
                <button
                  onClick={getExplorerStatus}
                  disabled={loading || isProcessing || isExecuting}
                  className="flex items-center gap-1.5 px-2 py-0.5 bg-gray-800 hover:bg-gray-700 text-gray-300 rounded text-[10px] transition disabled:opacity-50"
                  title="Refresh Context"
                >
                  {loading ? <Loader2 className="animate-spin" size={10} /> : <Terminal size={10} className="rotate-180" />}
                  <span>Sync</span>
                </button>
              </div>
              <div className="font-mono text-gray-300 bg-black/30 p-2 rounded border border-gray-800 truncate">
                {explorerState.path}
              </div>
            </div>
            <div>
              <span className="text-gray-500 uppercase font-bold tracking-wider text-[10px] flex items-center justify-between">
                <span>Selected Files</span>
                <span className="bg-gray-800 px-1.5 rounded text-gray-400">{explorerState.selected_files.length}</span>
              </span>
              {explorerState.selected_files.length > 0 ? (
                <ul className="mt-1 space-y-1 max-h-24 overflow-y-auto custom-scrollbar p-1">
                  {explorerState.selected_files.map((file, i) => (
                    <li key={i} className="flex items-start gap-2 text-gray-400 hover:text-gray-200">
                      <File size={12} className="mt-0.5 shrink-0" />
                      <span className="break-all">{file}</span>
                    </li>
                  ))}
                </ul>
              ) : <div className="mt-1 text-gray-600 italic">No selection</div>}
            </div>
          </div>
        ) : (
          <div className="p-8 flex flex-col items-center justify-center gap-3 text-xs">
            <span className="text-gray-500">No active context found.</span>
            <button
              onClick={getExplorerStatus}
              disabled={loading || isProcessing || isExecuting}
              className="flex items-center gap-2 px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white rounded-md transition disabled:opacity-50 text-xs font-medium"
            >
              {loading ? <Loader2 className="animate-spin" size={14} /> : <Terminal size={14} className="rotate-180" />}
              <span>Sync Context</span>
            </button>
          </div>
        )}
      </div>

      {/* Chat Area */}
      <section className="flex-1 flex flex-col relative overflow-hidden">
        {chatHistory.length === 0 ? (
          <div className="flex-1 flex flex-col items-center justify-center opacity-30 select-none">
            <Bot size={48} />
            <p className="mt-2 text-sm font-medium">Agent Ready</p>
          </div>
        ) : (
          <div className="flex-1 overflow-y-auto p-4 space-y-6 custom-scrollbar scroll-smooth">
            {chatHistory.map((msg, idx) => {
              if (msg.role === 'system') {
                return (
                  <div key={idx} className="flex justify-center my-4 group">
                    <details className="bg-gray-900 border border-gray-800 rounded-lg text-xs max-w-2xl w-full open:bg-gray-900/50 transition-all">
                      <summary className="px-4 py-2 cursor-pointer text-gray-500 hover:text-gray-300 font-medium select-none flex items-center gap-2 list-none">
                        <Info size={14} className="opacity-70" />
                        <span>System Context</span>
                        <span className="ml-auto text-[10px] opacity-40">Click to expand</span>
                      </summary>
                      <div className="p-4 pt-0 text-gray-400 font-mono whitespace-pre-wrap border-t border-gray-800/50 mt-2">
                        {msg.content}
                      </div>
                    </details>
                  </div>
                );
              }

              const isExpanded = expandedResponses[idx];

              return (
                <div key={idx} className={`flex gap-3 ${msg.role === 'user' ? 'flex-row-reverse' : ''} group`}>
                  <div className={`w-6 h-6 rounded flex items-center justify-center shrink-0 mt-0.5 ${msg.role === 'user' ? 'bg-indigo-600 text-white' :
                    msg.role === 'tool' ? 'bg-gray-800 text-emerald-500' : 'bg-emerald-600 text-white'
                    }`}>
                    {msg.role === 'user' ? <User size={12} /> : msg.role === 'tool' ? <Terminal size={12} /> : <Bot size={12} />}
                  </div>

                  <div className={`max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed shadow-sm relative ${msg.role === 'user'
                    ? 'bg-[#1e1e2e] text-indigo-100 border border-indigo-500/10'
                    : msg.role === 'tool'
                      ? 'bg-black/40 text-green-400 font-mono border border-gray-800 text-xs w-full overflow-hidden'
                      : 'text-gray-300'
                    }`}>
                    {msg.role === 'tool' ? (
                      <div className="flex flex-col">
                        <div className={`whitespace-pre-wrap ${!isExpanded ? 'max-h-32 overflow-hidden relative' : ''}`}>
                          {msg.content}
                          {!isExpanded && msg.content.length > 200 && (
                            <div className="absolute bottom-0 left-0 right-0 h-8 bg-gradient-to-t from-gray-950/80 to-transparent pointer-events-none" />
                          )}
                        </div>
                        {msg.content.length > 200 && (
                          <button
                            onClick={() => toggleExpand(idx)}
                            className="mt-2 flex items-center gap-1 text-[10px] text-gray-500 hover:text-gray-300 uppercase font-bold tracking-wider"
                          >
                            {isExpanded ? <><ChevronUp size={10} /> Show Less</> : <><ChevronDown size={10} /> Show More</>}
                          </button>
                        )}
                      </div>
                    ) : (
                      <div className="flex flex-col">
                        <div className={`prose prose-invert prose-xs max-w-none
                                        prose-p:leading-snug prose-p:my-1 prose-headings:my-2 prose-li:my-0.5
                                        prose-pre:bg-black/50 prose-pre:border prose-pre:border-white/10 prose-pre:text-[11px] prose-pre:p-2
                                        prose-code:text-indigo-300 prose-code:bg-white/5 prose-code:px-1 prose-code:rounded prose-code:before:content-none prose-code:after:content-none
                                        ${!isExpanded && msg.content.length > 500 ? 'max-h-48 overflow-hidden relative' : ''}`}>
                          <ReactMarkdown>
                            {msg.content}
                          </ReactMarkdown>
                          {!isExpanded && msg.content.length > 500 && (
                            <div className="absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-gray-950 to-transparent pointer-events-none" />
                          )}
                        </div>
                        {msg.content.length > 500 && (
                          <button
                            onClick={() => toggleExpand(idx)}
                            className="mt-2 flex items-center gap-1 text-[10px] text-gray-500 hover:text-gray-300 uppercase font-bold tracking-wider self-start"
                          >
                            {isExpanded ? <><ChevronUp size={10} /> Show Less</> : <><ChevronDown size={10} /> Show More</>}
                          </button>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              )
            })}

            {/* Thinking / Running Indicator */}
            {(isProcessing || isExecuting) && !pendingCommand && (
              <div className="flex gap-3 pl-1 animate-in fade-in duration-300 items-center">
                <div className={`w-6 h-6 rounded flex items-center justify-center shrink-0 animate-pulse ${isExecuting ? 'bg-indigo-600/20 text-indigo-500' : 'bg-emerald-600/20 text-emerald-500'}`}>
                  {isExecuting ? <Terminal size={12} /> : <Bot size={12} />}
                </div>
                <div className="flex items-center gap-2 text-gray-500 text-xs py-1">
                  <Loader2 className="animate-spin" size={12} />
                  <span>{isExecuting ? 'Executing command...' : 'Thinking...'}</span>
                </div>
                <button
                  onClick={stopAgent}
                  className="ml-2 p-1 rounded text-red-500 hover:bg-red-500/20 hover:text-red-400 transition"
                  title="Stop agent"
                >
                  <StopCircle size={16} />
                </button>
              </div>
            )}
            <div ref={chatEndRef} />
          </div>
        )}
      </section>

      {/* Pending Command */}
      {pendingCommand && (
        <div className="flex-none p-3 pt-0">
          <div className="bg-black/40 border border-emerald-500/30 rounded-lg overflow-hidden">
            <div className="bg-emerald-500/10 px-3 py-1.5 flex items-center justify-between border-b border-emerald-500/20">
              <div className="flex items-center gap-2 text-emerald-400 text-[10px] font-bold uppercase tracking-wider">
                <Terminal size={12} />
                Command Proposal (editable)
              </div>
            </div>
            <div className="p-3">
              <textarea
                className="block w-full font-mono text-xs text-gray-300 bg-black/50 p-2 rounded border border-white/5 resize-none focus:outline-none focus:border-emerald-500/50 custom-scrollbar"
                rows={3}
                value={pendingCommand}
                onChange={(e) => setPendingCommand(e.target.value)}
                disabled={isExecuting}
              />
              <div className="flex gap-2 mt-3">
                <button
                  onClick={() => approveCommand(false)}
                  disabled={isProcessing || isExecuting || isFeedbackOpen}
                  className="flex-1 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-medium transition flex items-center justify-center gap-1.5 disabled:opacity-50"
                >
                  {isExecuting ? <Loader2 className="animate-spin" size={12} /> : <Play size={12} />}
                  {isExecuting ? 'Running...' : 'Execute'}
                </button>
                <button
                  onClick={() => {
                    setAutoExecute(true);
                    approveCommand(true);
                  }}
                  disabled={isProcessing || isExecuting || isFeedbackOpen}
                  className="px-3 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-medium transition flex items-center justify-center gap-1.5 disabled:opacity-50"
                  title="Execute this and all following commands automatically"
                >
                  <Play size={12} />
                  <Play size={12} className="-ml-2" />
                  All
                </button>
                <button
                  onClick={() => {
                    if (isFeedbackOpen && feedbackText.trim()) {
                      const newHistory = [...chatHistory, { role: "user", content: `I don't want to run that command. ${feedbackText}` }];
                      setChatHistory(newHistory);
                      setPendingCommand(null);
                      setIsFeedbackOpen(false);
                      setFeedbackText("");
                      setIsProcessing(true);
                      runAgentStep(newHistory);
                    } else {
                      setIsFeedbackOpen(!isFeedbackOpen);
                    }
                  }}
                  disabled={isProcessing || isExecuting}
                  className={`px-4 py-1.5 rounded text-xs font-medium transition flex items-center gap-1.5 disabled:opacity-50 ${isFeedbackOpen ? 'bg-amber-500 text-white hover:bg-amber-400' : 'bg-amber-600/20 text-amber-500 border border-amber-600/30 hover:bg-amber-600/30'}`}
                >
                  <Info size={12} />
                  {isFeedbackOpen ? 'Send Feedback' : 'Reject with Feedback'}
                </button>
                <button
                  onClick={() => {
                    setPendingCommand(null);
                    setIsProcessing(false);
                    setIsFeedbackOpen(false);
                    setFeedbackText("");
                  }}
                  disabled={isProcessing || isExecuting}
                  className="px-4 py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded text-xs font-medium transition flex items-center gap-1.5 disabled:opacity-50"
                >
                  <X size={12} />
                  Dismiss
                </button>
              </div>

              {/* Inline Feedback Textarea */}
              {isFeedbackOpen && (
                <div className="mt-3 pt-3 border-t border-white/5 animate-in slide-in-from-top-2">
                  <textarea
                    className="w-full bg-amber-950/20 text-amber-100 placeholder-amber-700/50 text-xs p-2 rounded border border-amber-500/20 focus:outline-none focus:border-amber-500/50 resize-none font-sans"
                    placeholder="Tell the agent what to fix..."
                    rows={2}
                    value={feedbackText}
                    onChange={(e) => setFeedbackText(e.target.value)}
                    autoFocus
                  />
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Bottom Input */}
      <div className="flex-none border-t border-gray-800 p-3 bg-gray-900/50">
        <div className="max-w-3xl mx-auto bg-gray-900 rounded-xl border border-gray-800 overflow-hidden focus-within:border-gray-700 transition">
          <input
            ref={inputRef}
            className="w-full bg-transparent text-sm text-gray-200 placeholder-gray-600 px-4 pt-3 pb-2 focus:outline-none"
            placeholder={pendingCommand ? "Approve command above..." : "Ask me anything..."}
            value={agentPrompt}
            onChange={(e) => setAgentPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') startAgent();
            }}
            disabled={isProcessing || isExecuting || pendingCommand !== null}
            autoFocus
          />
          <div className="flex items-center justify-end gap-2 px-3 pb-2">
            <select
              className={`bg-transparent text-[11px] rounded px-1 py-0.5 focus:outline-none cursor-pointer ${
                ollamaConnected === false ? 'text-red-400' :
                availableModels.length === 0 ? 'text-amber-400' :
                'text-gray-500 hover:text-gray-400'
              }`}
              value={modelName}
              onChange={(e) => setModelName(e.target.value)}
              disabled={isProcessing || isExecuting || availableModels.length === 0}
            >
              {ollamaConnected === null ? (
                <option value="">Checking...</option>
              ) : ollamaConnected === false ? (
                <option value="">No Ollama</option>
              ) : availableModels.length === 0 ? (
                <option value="">No models</option>
              ) : (
                availableModels.map(m => <option key={m} value={m}>{m}</option>)
              )}
            </select>
            <button
              onClick={() => startAgent()}
              disabled={isProcessing || isExecuting || pendingCommand !== null || !agentPrompt.trim()}
              className="w-7 h-7 flex items-center justify-center rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white transition disabled:opacity-30 disabled:hover:bg-indigo-600"
              title="Send"
            >
              <ArrowUp size={16} strokeWidth={2.5} />
            </button>
          </div>
        </div>
      </div>

      </div>

      {/* Error Toast */}
      {error && (
        <div className="absolute top-4 right-4 bg-red-900/95 text-red-200 text-xs px-3 py-2 rounded-lg border border-red-700 shadow-lg animate-in slide-in-from-right fade-in z-50 flex items-center gap-2 max-w-md">
          <span className="flex-1">{error}</span>
          <button
            onClick={() => setError("")}
            className="p-1 hover:bg-red-800 rounded transition shrink-0"
            title="Dismiss"
          >
            <X size={12} />
          </button>
        </div>
      )}
    </main>
  );
}

export default App;
