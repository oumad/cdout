import { useState, useEffect, useRef } from "react";
import ReactMarkdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Terminal,
  Info,
  File,
  Bot,
  Loader2,
  StopCircle,
  FolderSync,
  RefreshCw,
  Settings,
  X,
  CheckCircle,
  XCircle,
} from "lucide-react";
import "./App.css";
import type { Message, SpotlightSubmitPayload, ApiKeysResponse, Skill, CliCredentialsStatus } from "./types";
import {
  BLUR_GRACE_MS,
  FOCUS_DELAY_MS,
  AUTO_STEP_DELAY_MS,
  MAX_AUTO_STEPS,
  FILE_LIST_THRESHOLD,
  EVENTS,
} from "./constants";
import * as api from "./utils/tauri";
import { useModels, useExplorer, useError } from "./hooks";
import { isTaskComplete } from "./utils/agent";
import {
  ErrorToast,
  ModelSelector,
  TitleBar,
  ChatMessage,
  CommandApproval,
  ChatInput,
} from "./components";

const WINDOW_LABEL = getCurrentWindow().label;

function App() {
  if (WINDOW_LABEL === "spotlight") return <SpotlightApp />;
  return <MainApp />;
}

// ═══════════════════════════════════════════════════════════════
// SpotlightApp — lightweight input window
// ═══════════════════════════════════════════════════════════════

function SpotlightApp() {
  const { modelName, setModelName, availableModels, ollamaConnected } = useModels();
  const { explorerState, fetchExplorer } = useExplorer();
  const { error, showError, clearError } = useError();
  const [agentPrompt, setAgentPrompt] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const shownAt = useRef(0);

  useEffect(() => {
    fetchExplorer();
  }, [fetchExplorer]);

  useEffect(() => {
    const unlisten = getCurrentWindow().onFocusChanged(async ({ payload: focused }) => {
      if (focused) {
        shownAt.current = Date.now();
        fetchExplorer();
        setTimeout(() => inputRef.current?.focus(), FOCUS_DELAY_MS);
      } else {
        if (Date.now() - shownAt.current > BLUR_GRACE_MS) {
          getCurrentWindow().hide();
        }
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, [fetchExplorer]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") getCurrentWindow().hide();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  async function submit() {
    if (!agentPrompt.trim()) return;
    try {
      await api.spotlightSubmit(agentPrompt, modelName);
      setAgentPrompt("");
    } catch (e) {
      showError(String(e));
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
            onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
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
              <ModelSelector
                modelName={modelName}
                onChange={setModelName}
                models={availableModels}
                connected={ollamaConnected}
                className="text-xs text-gray-600"
              />
              <span className="text-gray-700">·</span>
              <span>Esc to dismiss</span>
            </div>
          </div>
        </div>
      </div>

      <ErrorToast message={error} onDismiss={clearError} />
    </main>
  );
}

// ═══════════════════════════════════════════════════════════════
// MainApp — full conversation UI
// ═══════════════════════════════════════════════════════════════

function MainApp() {
  const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");
  const { modelName, setModelName, availableModels, ollamaConnected, setOllamaConnected, fetchModels } = useModels(ollamaUrl);
  const { explorerState, setExplorerState } = useExplorer();
  const { error, showError, clearError } = useError();
  const [loading, setLoading] = useState(false);

  // Settings State
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [tempOllamaUrl, setTempOllamaUrl] = useState("");
  const [agentPrompt, setAgentPrompt] = useState("");
  const [antigravityEmail, setAntigravityEmail] = useState<string | null>(null);

  // Provider State
  const [openaiKey, setOpenaiKey] = useState("");
  const [geminiKey, setGeminiKey] = useState("");

  // Skills State
  const [skills, setSkills] = useState<Skill[]>([]);

  // CLI Credentials State
  const [cliCredentials, setCliCredentials] = useState<CliCredentialsStatus | null>(null);

  // Conversation State
  const [chatHistory, setChatHistory] = useState<Message[]>([]);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [autoExecute, setAutoExecute] = useState(false);
  const autoExecuteRef = useRef(autoExecute);
  autoExecuteRef.current = autoExecute;
  const stopRequested = useRef(false);
  const autoStepCount = useRef(0);

  const inputRef = useRef<HTMLInputElement>(null);

  // Streaming State
  const [streamingText, setStreamingText] = useState("");

  // UI State
  const [isContextOpen, setIsContextOpen] = useState(false);
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
        const url = await api.getOllamaUrl();
        setOllamaUrl(url);
        setTempOllamaUrl(url);
      } catch (err) {
        console.error("Failed to load Ollama URL:", err);
      }
    };
    loadUrl();
    api.getAntigravityStatus().then(setAntigravityEmail).catch(console.error);
    api.listSkills().then(setSkills).catch(console.error);
    api.getCliCredentialsStatus().then(setCliCredentials).catch(console.error);
    api.getApiKeys().then((keys: ApiKeysResponse) => {
      setOpenaiKey(keys.openai || "");
      setGeminiKey(keys.gemini || "");
    }).catch(console.error);
  }, []);

  // Listen for spotlight submission
  useEffect(() => {
    const unlisten = listen<SpotlightSubmitPayload>(EVENTS.SPOTLIGHT_SUBMITTED, async (event) => {
      const { prompt, model } = event.payload;
      setModelName(model);
      setIsProcessing(true);
      try {
        const state = await api.getExplorerStatus();
        setExplorerState(state);
        const history = await api.initAgentConversation(
          state.path,
          state.selected_files,
          prompt
        );
        lastSyncedContext.current = { path: state.path, files: [...state.selected_files] };
        setSyncPending(false);
        setChatHistory(history);

        await runAgentStep(history, autoExecuteRef.current, model);
      } catch (e) {
        showError(String(e));
        setIsProcessing(false);
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Auto-scroll to bottom
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chatHistory, pendingCommand, isProcessing, isExecuting, streamingText]);

  async function getExplorerStatus() {
    setLoading(true);
    try {
      const state = await api.getExplorerStatus();
      setExplorerState(state);
      clearError();

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
      showError(String(e));
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
        history = await api.initAgentConversation(
          explorerState.path,
          explorerState.selected_files,
          promptToSend
        );
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

          if (explorerState.selected_files.length > FILE_LIST_THRESHOLD) {
            const fileListPath = await api.writeFileList(explorerState.selected_files);
            contextUpdate = `[CONTEXT UPDATE - Files have changed]\n\nWorking directory: ${explorerState.path}\nSelected files: ${explorerState.selected_files.length} files (too many to list inline)\n\nThe complete file list has been written to: ${fileListPath}\n\nTo read the file list in PowerShell, use:\n$files = Get-Content '${fileListPath}'\n\nPlease use these updated files for any subsequent operations.`;
          } else {
            const filesList = explorerState.selected_files.length > 0
              ? explorerState.selected_files.map((f, i) => `${i + 1}. ${f}`).join('\n')
              : 'No specific files selected';
            contextUpdate = `[CONTEXT UPDATE - Files have changed]\n\nWorking directory: ${explorerState.path}\nSelected files (${explorerState.selected_files.length} total):\n${filesList}\n\nPlease use these updated files for any subsequent operations.`;
          }

          newHistory.push({ role: "user" as const, content: contextUpdate });

          lastSyncedContext.current = {
            path: explorerState.path,
            files: [...explorerState.selected_files]
          };
          setSyncPending(false);
        }

        newHistory.push({ role: "user" as const, content: promptToSend });
        setChatHistory(newHistory);
        await runAgentStep(newHistory);
      }

    } catch (e) {
      console.error(`Error starting conversation: ${e}`);
      showError(String(e));
      setIsProcessing(false);
    }
  }

  // Step 2: Run Agent Step
  async function runAgentStep(history: Message[], shouldAutoExecute = autoExecute, modelOverride?: string) {
    try {
      setStreamingText("");
      const result = await api.runAgentStepStream(
        modelOverride || modelName,
        history,
        (chunk) => {
          if (chunk.kind === "TextDelta") {
            setStreamingText((prev) => prev + chunk.text);
          }
        }
      );
      setStreamingText("");

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
            const output = await api.executePowershell(cmd, explorerState?.path || null);
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
          const taskDone = isTaskComplete(result.response.content);

          if (!taskDone && autoStepCount.current < MAX_AUTO_STEPS) {
            autoStepCount.current++;
            setTimeout(async () => {
              if (stopRequested.current) return;
              const newHistory = [...result.updated_history, { role: "user" as const, content: "If there are remaining UNFINISHED steps, proceed to the next one. If all steps are already done, say 'Task Complete'. Do NOT repeat any step that has already been executed." }];
              setChatHistory(newHistory);
              setIsProcessing(true);
              await runAgentStep(newHistory, true, modelOverride);
            }, AUTO_STEP_DELAY_MS);
            return;
          }
        }

        setAutoExecute(false);
      }
    } catch (e) {
      console.error(`Error running agent step: ${e}`);
      showError(`LLM Error: ${String(e)}`);
      setStreamingText("");
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

    if (continueAuto) setAutoExecute(true);

    setIsExecuting(true);
    try {
      const output = await api.executePowershell(pendingCommand, explorerState?.path || null);

      const newMsg: Message = { role: "tool", content: output };
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

  function rejectCommand(feedback: string) {
    const newHistory: Message[] = [...chatHistory, { role: "user" as const, content: `I don't want to run that command. ${feedback}` }];
    setChatHistory(newHistory);
    setPendingCommand(null);
    setIsProcessing(true);
    runAgentStep(newHistory);
  }

  function dismissCommand() {
    setPendingCommand(null);
    setIsProcessing(false);
  }

  function toggleExpand(idx: number) {
    setExpandedResponses(prev => ({ ...prev, [idx]: !prev[idx] }));
  }

  const handleAntigravityLogin = async () => {
    try {
      const email = await api.loginAntigravity();
      setAntigravityEmail(email);
    } catch (e) {
      showError("Login Failed: " + String(e));
    }
  };

  const saveApiKeys = async () => {
    try {
      await api.setOpenaiKey(openaiKey);
      await api.setGeminiKey(geminiKey);
      setIsSettingsOpen(false);
      fetchModels();
    } catch (e) {
      showError("Failed to save keys: " + String(e));
    }
  };

  return (
    <main className="flex flex-col h-screen text-gray-100 font-sans overflow-hidden">
      <div className="flex flex-col h-full bg-gray-950">

      <TitleBar />

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
                  {cliCredentials && (
                    <div className="pt-2 border-t border-gray-800 space-y-1.5">
                      <label className="block text-xs text-gray-400 mb-1">CLI Providers</label>
                      <div className={`flex items-center justify-between px-2 py-1.5 rounded text-xs ${
                        cliCredentials.claude_code
                          ? 'bg-emerald-900/20 border border-emerald-500/20'
                          : 'bg-gray-800/50 border border-gray-700/50'
                      }`}>
                        <span className={cliCredentials.claude_code ? 'text-emerald-400' : 'text-gray-500'}>
                          Claude Code
                        </span>
                        {cliCredentials.claude_code ? (
                          <CheckCircle size={12} className="text-emerald-500" />
                        ) : (
                          <span className="text-[10px] text-gray-600">Not found</span>
                        )}
                      </div>
                      <div className={`flex items-center justify-between px-2 py-1.5 rounded text-xs ${
                        cliCredentials.codex
                          ? 'bg-emerald-900/20 border border-emerald-500/20'
                          : 'bg-gray-800/50 border border-gray-700/50'
                      }`}>
                        <span className={cliCredentials.codex ? 'text-emerald-400' : 'text-gray-500'}>
                          Codex CLI
                        </span>
                        {cliCredentials.codex ? (
                          <CheckCircle size={12} className="text-emerald-500" />
                        ) : (
                          <span className="text-[10px] text-gray-600">Not found</span>
                        )}
                      </div>
                    </div>
                  )}
                  {skills.length > 0 && (
                    <div className="pt-2 border-t border-gray-800 space-y-1.5">
                      <label className="block text-xs text-gray-400 mb-1">Skills</label>
                      {skills.map((skill) => (
                        <div
                          key={skill.metadata.name}
                          className={`flex items-center justify-between px-2 py-1.5 rounded text-xs ${
                            skill.available
                              ? 'bg-emerald-900/20 border border-emerald-500/20'
                              : 'bg-gray-800/50 border border-gray-700/50'
                          }`}
                        >
                          <div className="flex flex-col">
                            <span className={skill.available ? 'text-emerald-400' : 'text-gray-500'}>
                              {skill.metadata.name}
                            </span>
                            <span className="text-[10px] text-gray-600">{skill.metadata.description}</span>
                          </div>
                          {skill.available ? (
                            <CheckCircle size={12} className="text-emerald-500 shrink-0" />
                          ) : (
                            <div className="flex items-center gap-1 shrink-0">
                              <XCircle size={12} className="text-gray-600" />
                              <span className="text-[10px] text-gray-600">
                                {skill.missing_bins.join(', ')}
                              </span>
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                  <div className="flex gap-2 pt-1 border-t border-gray-800 mt-2">
                    <button
                      onClick={async () => {
                        try {
                          await api.setOllamaUrl(tempOllamaUrl);
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
            {chatHistory.map((msg, idx) => (
              <ChatMessage
                key={idx}
                message={msg}
                index={idx}
                isExpanded={!!expandedResponses[idx]}
                onToggle={toggleExpand}
              />
            ))}

            {/* Streaming bubble */}
            {streamingText && (
              <div className="flex gap-3 group">
                <div className="w-6 h-6 rounded flex items-center justify-center shrink-0 mt-0.5 bg-emerald-600 text-white">
                  <Bot size={12} />
                </div>
                <div className="max-w-[85%] text-sm leading-relaxed text-gray-300">
                  <div className="prose prose-invert prose-xs max-w-none prose-p:leading-snug prose-p:my-1 prose-headings:my-2 prose-li:my-0.5 prose-pre:bg-black/50 prose-pre:border prose-pre:border-white/10 prose-pre:text-[11px] prose-pre:p-2 prose-code:text-indigo-300 prose-code:bg-white/5 prose-code:px-1 prose-code:rounded prose-code:before:content-none prose-code:after:content-none">
                    <ReactMarkdown>{streamingText}</ReactMarkdown>
                  </div>
                  <span className="inline-block w-1.5 h-4 bg-emerald-500 animate-pulse ml-0.5 align-text-bottom" />
                </div>
              </div>
            )}

            {(isProcessing || isExecuting) && !pendingCommand && !streamingText && (
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

      {pendingCommand && (
        <CommandApproval
          command={pendingCommand}
          onChange={setPendingCommand}
          onApprove={approveCommand}
          onReject={rejectCommand}
          onDismiss={dismissCommand}
          isExecuting={isExecuting}
          isProcessing={isProcessing}
        />
      )}

      <ChatInput
        ref={inputRef}
        prompt={agentPrompt}
        onChange={setAgentPrompt}
        onSubmit={startAgent}
        disabled={isProcessing || isExecuting || pendingCommand !== null}
        hasPendingCommand={pendingCommand !== null}
        modelName={modelName}
        onModelChange={setModelName}
        models={availableModels}
        ollamaConnected={ollamaConnected}
      />

      </div>

      <ErrorToast message={error} onDismiss={clearError} />
    </main>
  );
}

export default App;
