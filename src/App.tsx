import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
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
  Settings
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

function App() {
  const [explorerState, setExplorerState] = useState<ExplorerState | null>(null);
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState(false);

  // Agent State
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null); // null = checking, false = disconnected, true = connected
  const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [tempOllamaUrl, setTempOllamaUrl] = useState("");
  const [agentPrompt, setAgentPrompt] = useState("");
  const [antigravityEmail, setAntigravityEmail] = useState<string | null>(null);

  // New Provider State
  const [openaiKey, setOpenaiKey] = useState("");
  const [geminiKey, setGeminiKey] = useState("");
  // const [apiKeysLoaded, setApiKeysLoaded] = useState(false);

  // useEffect(() => {
  //   invoke<string | null>("get_antigravity_status").then(setAntigravityEmail).catch(console.error);
  // }, []);

  // const handleAntigravityLogin = async () => {
  //   try {
  //     const email = await invoke<string>("login_antigravity");
  //     setAntigravityEmail(email);
  //   } catch (e) {
  //     setError("Login Failed: " + String(e));
  //   }
  // };

  // Conversation State
  const [chatHistory, setChatHistory] = useState<Message[]>([]);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [autoExecute, setAutoExecute] = useState(false);
  const stopRequested = useRef(false);

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
    // Poll every 5 seconds if Ollama is not connected or has no models
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
      // Debug: log all explorer windows/tabs to console
      const debugInfo = await invoke<any>("get_explorer_debug");
      console.log("Explorer Debug Info:", debugInfo);

      const state = await invoke<ExplorerState>("get_explorer_status");
      setExplorerState(state);
      setError("");

      // If we already have a conversation, mark that context needs to be updated
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
    setAgentPrompt(""); // Clear input immediately
    setIsProcessing(true);
    setPendingCommand(null);

    try {
      let history = chatHistory;

      if (history.length === 0) {
        // First message - initialize with full context
        history = await invoke<Message[]>("init_agent_conversation", {
          contextPath: explorerState.path,
          selectedFiles: explorerState.selected_files,
          userPrompt: promptToSend
        });
        // Track the synced context
        lastSyncedContext.current = {
          path: explorerState.path,
          files: [...explorerState.selected_files]
        };
        setSyncPending(false);
        setChatHistory(history);
        await runAgentStep(history);
      } else {
        // Follow-up message
        let newHistory = [...history];

        // If sync is pending, inject context update before user message
        if (syncPending && explorerState) {
          let contextUpdate: string;

          // For large file lists (>20), use file-based mode
          const FILE_LIST_THRESHOLD = 20;
          if (explorerState.selected_files.length > FILE_LIST_THRESHOLD) {
            // Write file list to temp file
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

          // Track the new synced context
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
  async function runAgentStep(history: Message[], shouldAutoExecute = autoExecute) {
    try {
      const result = await invoke<AgentStepResult>("run_agent_step", {
        model: modelName,
        history: history
      });

      // Update history (includes the assistant's reply/tool call)
      setChatHistory(result.updated_history);

      // Check if stop was requested
      if (stopRequested.current) {
        stopRequested.current = false;
        return;
      }

      if (result.response.type === "CommandProposal") {
        const cmd = result.response.content;
        if (shouldAutoExecute) {
          // Auto-execute without asking
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
            await runAgentStep(newHistory, true);
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

        // Auto-Continue Logic
        if (shouldAutoExecute) {
          const contentLower = result.response.content.toLowerCase();
          // Simple heuristic: if it says "done", "complete", "finished", or "success", we stop.
          // Otherwise, we assume it's an intermediate step or explanation and nudge it to continue.
          const isTaskDone =
            contentLower.includes("task complete") ||
            contentLower.includes("all done") ||
            contentLower.includes("successfully processed all") ||
            (contentLower.includes("finished") && contentLower.length < 100);

          if (!isTaskDone) {
            // It's just talking, but we want it to keep working if there's more to do.
            // We inject a hidden "proceed" message to keep the loop alive.
            // Wait a small delay to avoid UI jitter
            setTimeout(async () => {
              if (stopRequested.current) return;
              const newHistory = [...result.updated_history, { role: "user", content: "If there are more steps, proceed. Otherwise say 'Task Complete'." }];
              setChatHistory(newHistory);
              setIsProcessing(true);
              await runAgentStep(newHistory, true);
            }, 500);
            return;
          }
        }

        setAutoExecute(false); // Reset auto-execute when done
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

      // Append Tool Result to History
      const newMsg: Message = {
        role: "tool",
        content: output
      };
      const newHistory = [...chatHistory, newMsg];
      setChatHistory(newHistory);
      setPendingCommand(null); // Clear pending
      setIsExecuting(false);

      // Continue Loop
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
      fetchOllamaModels(); // Refresh list
    } catch (e) {
      setError("Failed to save keys: " + String(e));
    }
  };

  return (
    <main className="flex flex-col h-screen bg-gray-950 text-gray-100 font-sans overflow-hidden">

      {/* Top Command Palette Bar */}
      <header className="flex-none h-16 bg-gray-900 border-b border-gray-800 flex items-center gap-4 px-4 select-none drag-region">

        {/* Visual Anchor */}
        <div className="flex-none text-indigo-500 bg-indigo-500/10 p-2 rounded-lg">
          <Terminal size={20} />
        </div>

        {/* Floating Sync Badge */}
        <div
          className="flex-none relative no-drag"
          onMouseEnter={() => setShowSyncTooltip(true)}
          onMouseLeave={() => setShowSyncTooltip(false)}
        >
          <button
            onClick={getExplorerStatus}
            disabled={loading || isProcessing || isExecuting}
            className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-all disabled:opacity-50 ${syncPending
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
              <span className="tabular-nums">
                {explorerState.selected_files.length}
              </span>
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
                  ⚡ Context will update with next message
                </div>
              )}
            </div>
          )}
        </div>

        {/* Main Input Field (Command Palette Style) */}
        <div className="flex-1 relative group no-drag">
          <input
            className="w-full bg-transparent text-lg text-gray-200 placeholder-gray-600 focus:outline-none font-medium h-full py-2"
            placeholder={pendingCommand ? "Please approve command below..." : "Ask me anything..."}
            value={agentPrompt}
            onChange={(e) => setAgentPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                startAgent();
              }
            }}
            disabled={isProcessing || isExecuting || pendingCommand !== null}
            autoFocus
          />
        </div>

        {/* Actions */}
        <div className="flex-none flex items-center gap-2 no-drag">
          {/* New Chat Button */}
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
              className="p-2 rounded-md text-gray-500 hover:text-gray-300 hover:bg-gray-800/50 transition"
              title="New Chat"
            >
              <X size={18} />
            </button>
          )}

          {/* Context Info Toggle */}
          <button
            onClick={() => setIsContextOpen(!isContextOpen)}
            className={`p-2 rounded-md transition ${isContextOpen ? 'bg-gray-800 text-gray-200' : 'text-gray-500 hover:text-gray-300 hover:bg-gray-800/50'}`}
            title="Toggle Context Details"
          >
            <Info size={18} />
          </button>

          {/* Model Selector */}
          <select
            className={`bg-gray-950 border text-[10px] rounded p-1 focus:ring-1 focus:ring-indigo-500 focus:outline-none w-24 ${ollamaConnected === false ? 'border-red-500/50 text-red-400' :
              availableModels.length === 0 ? 'border-amber-500/50 text-amber-400' :
                'border-gray-700 text-gray-400'
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

          {/* Settings Button */}
          <div className="relative">
            <button
              onClick={() => {
                setTempOllamaUrl(ollamaUrl);
                setIsSettingsOpen(!isSettingsOpen);
              }}
              className={`p-2 rounded-md transition ${isSettingsOpen ? 'bg-gray-800 text-gray-200' : 'text-gray-500 hover:text-gray-300 hover:bg-gray-800/50'}`}
              title="Settings"
            >
              <Settings size={18} />
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
                          // setIsSettingsOpen(false); // saveApiKeys handles this
                          // fetchOllamaModels(); // saveApiKeys handles this
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
      </header>

      {/* Collapsible Context Drawer */}
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

      {/* Main Chat Area */}
      <section className="flex-1 flex flex-col relative overflow-hidden bg-gray-950">
        {chatHistory.length === 0 ? (
          // Empty State - Minimal
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

        {/* Pending Command Overlay (Bottom) */}
        {pendingCommand && (
          <div className="p-4 bg-gray-900 border-t border-gray-800 animate-in slide-in-from-bottom-5">
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
      </section>

      {/* Error Toast */}
      {error && (
        <div className="absolute top-16 right-4 bg-red-900/90 text-red-200 text-xs px-3 py-2 rounded border border-red-700 shadow-lg animate-in slide-in-from-right fade-in z-50 flex items-center gap-2 max-w-md">
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
