import { useState, useEffect, useRef, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Terminal,
  Info,
  File,
  Bot,
  Loader2,
  FolderSync,
  RefreshCw,
  Settings,
  X,
  Zap,
} from "lucide-react";
import "./App.css";
import type {
  Message,
  SpotlightSubmitPayload,
  ApiKeysResponse,
  ProviderStatus,
  Skill,
  QuarantinedSkill,
  PendingQuestion,
  ToolProposal,
  ApprovalMode,
  CommandRisk,
} from "./types";
import {
  BLUR_GRACE_MS,
  FOCUS_DELAY_MS,
  AUTO_STEP_DELAY_MS,
  MAX_AUTO_STEPS,
  FILE_LIST_THRESHOLD,
  MODEL_POLL_MS,
  EVENTS,
  TOOLS,
} from "./constants";
import * as api from "./utils/tauri";
import { useModels, useExplorer, useError, useSessions, usePlatform } from "./hooks";
import { isTaskComplete } from "./utils/agent";
import {
  ErrorToast,
  ModelSelector,
  TitleBar,
  ChatMessage,
  CommandApproval,
  QuestionApproval,
  ChatInput,
  SettingsPage,
  MigrationBanner,
  ProviderSetup,
  SessionsSidebar,
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
  const { modelName, setModelName, availableModels, ollamaConnected, fetchModels } = useModels();
  const { explorerState, fetchExplorer } = useExplorer();
  const platform = usePlatform();
  const { error, showError, clearError } = useError();
  const [agentPrompt, setAgentPrompt] = useState("");
  const [isRefreshingModels, setIsRefreshingModels] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const shownAt = useRef(0);

  const refreshModels = async () => {
    setIsRefreshingModels(true);
    try {
      await fetchModels();
    } finally {
      setIsRefreshingModels(false);
    }
  };

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

  async function submit(autoApprove = false) {
    if (!agentPrompt.trim()) return;
    if (!modelName) {
      // No usable model yet — sending would fail in the router. Point the user
      // at the main window, where the first-run setup card lives.
      showError(
        "No model set up yet. Open cdout (from the tray) and pick a provider — local Ollama or a cloud key — to get started."
      );
      return;
    }
    try {
      await api.spotlightSubmit(agentPrompt, modelName, autoApprove);
      setAgentPrompt("");
    } catch (e) {
      showError(String(e));
    }
  }

  const spotlightPath = explorerState?.path || "";
  const spotlightFileCount = explorerState?.selected_files.length || 0;

  return (
    <main className="flex flex-col h-screen text-gray-100 font-sans overflow-hidden">
      <div className="flex-1 flex items-center justify-center drag-region">
        <div className="w-[560px] bg-gray-900 border border-gray-700/50 rounded-2xl shadow-2xl shadow-black/50 p-6 flex flex-col items-center gap-4 no-drag">
          <Bot size={36} className="text-indigo-500 opacity-80" />
          <input
            ref={inputRef}
            className="w-full bg-black/40 border border-gray-700/60 text-xl text-gray-200 placeholder-gray-600 rounded-xl px-5 py-3 focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/30 font-medium"
            placeholder="What would you like to do?"
            value={agentPrompt}
            onChange={(e) => setAgentPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              // Cmd/Ctrl+Enter pre-authorises the whole task, so the user
              // never has to wait for a first proposal just to click "All".
              submit(e.metaKey || e.ctrlKey);
            }}
            autoFocus
          />
          <div className="flex flex-col items-center gap-1 text-xs text-gray-500">
            {explorerState && explorerState.path && (
              <div className="flex flex-col items-center gap-0.5">
                <div className="flex items-center gap-1.5">
                  <FolderSync size={12} className="text-gray-600 shrink-0" />
                  <span className="truncate max-w-96" title={spotlightPath}>{spotlightPath}</span>
                </div>
                {spotlightFileCount > 0 && (
                  <div
                    className="flex items-center gap-1 text-gray-400"
                    title={explorerState.selected_files.join("\n")}
                  >
                    <File size={10} className="shrink-0 text-gray-600" />
                    {spotlightFileCount <= 2 ? (
                      <span className="truncate max-w-96">
                        {explorerState.selected_files.join(", ")}
                      </span>
                    ) : (
                      <span className="truncate max-w-96">
                        {explorerState.selected_files.slice(0, 2).join(", ")}
                        <span className="text-gray-600"> +{spotlightFileCount - 2} more</span>
                      </span>
                    )}
                  </div>
                )}
              </div>
            )}
            <span className="text-gray-600">
              <kbd className="font-mono">Enter</kbd> to run ·{" "}
              <kbd className="font-mono">
                {platform.os === "macos" ? "\u2318" : "Ctrl"}+Enter
              </kbd>{" "}
              to run without approving each step
            </span>
            {!spotlightPath && (
              // Otherwise an empty context area reads as "nothing selected"
              // when it can equally mean "no window open" or, on macOS, that
              // Automation access to Finder was never granted.
              <span className="text-gray-600">
                No {platform.file_manager} window detected
              </span>
            )}
            <div className="flex items-center gap-2 text-gray-600 max-w-full">
              <div className="flex items-center gap-1 min-w-0 max-w-[420px]">
                <ModelSelector
                  modelName={modelName}
                  onChange={setModelName}
                  models={availableModels}
                  connected={ollamaConnected}
                  className="text-xs text-gray-600 truncate min-w-0"
                  maxDisplayChars={32}
                />
                <button
                  onClick={refreshModels}
                  disabled={isRefreshingModels}
                  className="p-0.5 rounded text-gray-600 hover:text-gray-300 hover:bg-gray-800/50 transition disabled:opacity-50 shrink-0"
                  title="Refresh model list (re-checks Ollama + cloud keys)"
                  type="button"
                >
                  <RefreshCw
                    size={10}
                    className={isRefreshingModels ? "animate-spin" : ""}
                  />
                </button>
              </div>
              <span className="text-gray-700">·</span>
              <span className="whitespace-nowrap shrink-0">Esc to dismiss</span>
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
  const { error, showError, clearError } = useError();
  const { modelName, setModelName, availableModels, ollamaConnected, setOllamaConnected, fetchModels } = useModels(
    ollamaUrl,
    {
      onSwap: (prev, next) =>
        showError(
          `Model '${prev}' is no longer available — switched to '${next}'. Open Settings if you want to pick a different one.`
        ),
    }
  );
  const { explorerState, setExplorerState } = useExplorer();
  const platform = usePlatform();
  const [loading, setLoading] = useState(false);

  // Settings State
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [tempOllamaUrl, setTempOllamaUrl] = useState("");
  const [agentPrompt, setAgentPrompt] = useState("");

  // Provider State — single OpenRouter key, optional Anthropic for caching.
  // The values here only hold the user's CURRENT-edit-session input. The
  // server-side stored key is never read back into renderer state; the
  // placeholder UX shows a masked preview ("…3fa1") to confirm a key is set.
  const [openrouterKey, setOpenrouterKey] = useState("");
  const [anthropicKey, setAnthropicKey] = useState("");
  const [openrouterPreview, setOpenrouterPreview] = useState<string | null>(null);
  const [anthropicPreview, setAnthropicPreview] = useState<string | null>(null);
  const [showFreeOpenrouterModels, setShowFreeOpenrouterModels] = useState(false);

  // Skills State
  const [skills, setSkills] = useState<Skill[]>([]);
  const [quarantinedSkills, setQuarantinedSkills] = useState<QuarantinedSkill[]>([]);

  // One-time migration banner — shown when legacy CLI/Antigravity/openai/gemini
  // configs are detected on disk and the user hasn't acked yet.
  const [showMigrationBanner, setShowMigrationBanner] = useState(false);

  // First-run provider readiness — drives the onboarding setup card shown in
  // the empty chat state when no model is usable yet.
  const [providerStatus, setProviderStatus] = useState<ProviderStatus | null>(
    null
  );
  const refreshProviderStatus = useCallback(() => {
    api.getProviderStatus().then(setProviderStatus).catch(() => {});
  }, []);

  // Sessions — persistent chat history sidebar.
  const {
    sessions,
    currentSessionId,
    setCurrentSessionId,
    refresh: refreshSessions,
    newSession,
    loadSession: loadSessionFromStore,
    deleteSession: deleteSessionFromStore,
    renameSession: renameSessionInStore,
    scheduleSave,
    flushSave,
  } = useSessions();
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  // Conversation State
  const [chatHistory, setChatHistory] = useState<Message[]>([]);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);
  const [pendingQuestion, setPendingQuestion] = useState<PendingQuestion | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [isExecuting, setIsExecuting] = useState(false);
  const [autoExecute, setAutoExecute] = useState(false);
  // Persisted policy (default: run read-only commands unattended). Distinct
  // from `autoExecute`, which is this run's opt-in from "All" or a
  // modifier-submit and resets between tasks.
  const [approvalMode, setApprovalMode] = useState<ApprovalMode>("read_only");
  const approvalModeRef = useRef(approvalMode);
  approvalModeRef.current = approvalMode;
  // Risk of the surfaced proposal, so the approval card can say why it
  // stopped.
  const [pendingRisk, setPendingRisk] = useState<CommandRisk | null>(null);
  const autoExecuteRef = useRef(autoExecute);
  autoExecuteRef.current = autoExecute;
  const stopRequested = useRef(false);
  const autoStepCount = useRef(0);
  const consecutiveFailures = useRef(0);
  /**
   * Monotonic run-generation counter. Bumped whenever the current
   * conversation is torn down (session switch, new chat, spotlight submit).
   * Each runAgentStep captures the generation at entry and discards its
   * results after any await if the generation has moved on — this is the
   * robust fix for "a stale in-flight stream writes into the newly-loaded
   * session". A boolean flag reset after one microtask (the old approach)
   * could not cover a network round-trip that resolves later.
   */
  const runGeneration = useRef(0);
  /**
   * tool_call_id of the currently-surfaced command proposal. Needed so
   * dismiss/reject can append a correlated tool_result and never leave an
   * orphaned assistant tool_use (which Anthropic rejects with HTTP 400).
   */
  const pendingCommandToolId = useRef<string | undefined>(undefined);
  /**
   * True while an auto-step continuation is scheduled (setTimeout window) but
   * not yet started. Without this, the `finally` clears isProcessing during
   * the delay, the idle-detection effect sees the loop as idle, and a queued
   * prompt starts a SECOND concurrent agent loop.
   */
  const autoStepPending = useRef(false);
  /**
   * When the previous turn's loop_verdict was `Break`, set this so the NEXT
   * run_agent_step_stream call passes `drop_tools=true` — the backend then
   * sends no tool definitions, forcing the model into a text-only
   * reassessment. Auto-cleared after each turn is consumed.
   */
  const dropToolsForNextRequest = useRef(false);
  const [killDialog, setKillDialog] = useState<{ pid: number; command: string } | null>(null);
  const [queuedPrompts, setQueuedPrompts] = useState<string[]>([]);
  const queuedPromptsRef = useRef<string[]>([]);
  queuedPromptsRef.current = queuedPrompts;
  const MAX_CONSECUTIVE_FAILURES = 3;

  const inputRef = useRef<HTMLInputElement>(null);

  // Streaming State
  const [streamingText, setStreamingText] = useState("");

  // Persist conversation to the active session, debounced. Effect runs on
  // every chatHistory change while a session is selected.
  useEffect(() => {
    if (!currentSessionId) return;
    if (chatHistory.length === 0) return;
    scheduleSave(chatHistory);
  }, [chatHistory, currentSessionId, scheduleSave]);

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

  // Manual model-refresh state (button next to the picker in ChatInput).
  const [isRefreshingModels, setIsRefreshingModels] = useState(false);
  const refreshModels = async () => {
    setIsRefreshingModels(true);
    try {
      await fetchModels();
    } finally {
      setIsRefreshingModels(false);
    }
  };

  // Fetch saved Ollama URL on mount
  useEffect(() => {
    const loadUrl = async () => {
      try {
        const url = await api.getOllamaUrl();
        setOllamaUrl(url);
        setTempOllamaUrl(url);
        setApprovalMode(await api.getApprovalMode());
      } catch (err) {
        console.error("Failed to load Ollama URL:", err);
      }
    };
    loadUrl();
    api.listSkills()
      .then((r) => {
        setSkills(r.skills);
        setQuarantinedSkills(r.quarantined);
      })
      .catch(console.error);
    api
      .getApiKeys()
      .then((keys: ApiKeysResponse) => {
        // Plaintext is intentionally NOT pulled into state. Only the masked
        // preview is shown via placeholder until the user enters a new key.
        setOpenrouterPreview(keys.openrouter_preview);
        setAnthropicPreview(keys.anthropic_preview);
      })
      .catch(console.error);
    api.getShowFreeOpenrouterModels().then(setShowFreeOpenrouterModels).catch(() => {});

    // Migration banner: detect legacy creds + check whether user already acked.
    Promise.all([api.hasLegacyCredentials(), api.getOpenrouterDisclosureAck()])
      .then(([hasLegacy, acked]) => {
        if (hasLegacy && !acked) setShowMigrationBanner(true);
      })
      .catch(() => {});
  }, []);

  // Tear down any in-flight UI state. Called before starting a new session
  // (from spotlight, from the New-chat button, or from selecting a different
  // session in the sidebar) to guarantee no stale prompt/proposal/streaming
  // text leaks into the new view.
  async function hardResetSessionUi() {
    // Invalidate any in-flight agent step: the generation bump means its
    // post-await guards will discard its results instead of writing them into
    // the new session. This replaces the old (racy) "set stopRequested true,
    // await one microtask, set it false" dance, which could not survive a
    // network round-trip that resolved after the microtask.
    runGeneration.current++;
    stopRequested.current = false;
    autoStepPending.current = false;
    try {
      await api.cancelStream();
    } catch {
      // ignore — stream may not be active
    }
    try {
      await api.resetLoopDetector();
    } catch {
      // ignore
    }
    setStreamingText("");
    setPendingCommand(null);
    setPendingRisk(null);
    setPendingQuestion(null);
    pendingCommandToolId.current = undefined;
    setIsProcessing(false);
    setIsExecuting(false);
    setAutoExecute(false);
    autoStepCount.current = 0;
    consecutiveFailures.current = 0;
    setQueuedPrompts([]);
    setExpandedResponses({});
  }

  // Listen for spotlight submission. Every spotlight submit becomes a fresh
  // session — this is the documented fix for the "spotlight reuses stale main
  // window state" bug. The hard reset above guarantees the UI flips to a
  // clean state BEFORE the new session is loaded.
  useEffect(() => {
    const unlisten = listen<SpotlightSubmitPayload>(EVENTS.SPOTLIGHT_SUBMITTED, async (event) => {
      const { prompt, model, auto_approve } = event.payload;
      setIsSettingsOpen(false);
      try {
        // 1) Cancel any in-flight stream + clear all UI state.
        await flushSave();
        await hardResetSessionUi();
        // 2) Switch model context for the new session.
        setModelName(model);
        // 3) Fetch the current file-manager context.
        const state = await api.getExplorerStatus();
        setExplorerState(state);
        lastSyncedContext.current = {
          path: state.path,
          files: [...state.selected_files],
        };
        setSyncPending(false);
        // 4) Create a brand-new session (this writes to disk and gives us
        //    a clean history seeded with system+user messages).
        const session = await newSession(
          prompt,
          state.path,
          state.selected_files,
          model
        );
        setChatHistory(session.messages);
        // 5) Kick off the first agent step.
        setIsProcessing(true);
        // Pre-authorised from the spotlight: no waiting for a first proposal
        // just to click "All".
        if (auto_approve) setAutoExecute(true);
        await runAgentStep(
          session.messages,
          auto_approve || autoExecuteRef.current,
          model
        );
      } catch (e) {
        showError(String(e));
        setIsProcessing(false);
      }
    });
    return () => { unlisten.then(fn => fn()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Poll provider readiness ONLY while no model is usable. Stops the moment
  // the model list populates (Ollama came online or a key was saved), which
  // also unmounts the setup card. Mirrors the useModels poll cadence.
  useEffect(() => {
    if (availableModels.length > 0) return;
    refreshProviderStatus();
    const id = setInterval(refreshProviderStatus, MODEL_POLL_MS);
    return () => clearInterval(id);
  }, [availableModels.length, refreshProviderStatus]);

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

  // Submit if idle, otherwise queue until the current step finishes
  function submitOrQueue() {
    const text = agentPrompt.trim();
    if (!text) return;
    const isBusy =
      isProcessing ||
      isExecuting ||
      streamingText.length > 0 ||
      pendingCommand !== null ||
      pendingQuestion !== null ||
      autoStepPending.current;
    if (isBusy) {
      setQueuedPrompts((q) => [...q, text]);
      setAgentPrompt("");
    } else {
      startAgent();
    }
  }

  function removeQueuedPrompt(idx: number) {
    setQueuedPrompts((q) => q.filter((_, i) => i !== idx));
  }

  // Drain the queue when state goes idle
  useEffect(() => {
    const idle =
      !isProcessing &&
      !isExecuting &&
      !streamingText &&
      !pendingCommand &&
      !pendingQuestion &&
      !autoStepPending.current;
    if (idle && queuedPromptsRef.current.length > 0) {
      const next = queuedPromptsRef.current[0];
      setQueuedPrompts((q) => q.slice(1));
      queueMicrotask(() => startAgent(next));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isProcessing, isExecuting, streamingText, pendingCommand, pendingQuestion]);

  // Step 1: Initialize Conversation
  async function startAgent(promptOverride?: string) {
    const promptToSend = (promptOverride ?? agentPrompt).trim();
    if (!explorerState || !promptToSend) return;

    if (promptOverride === undefined) {
      setAgentPrompt("");
    }
    setIsProcessing(true);
    autoStepCount.current = 0;
    consecutiveFailures.current = 0;
    setPendingCommand(null);
    setPendingQuestion(null);
    // Backend auto-resets the loop detector when starting a fresh conversation,
    // but call it explicitly when the user kicks off via the "New chat" path
    // (where history is being thrown away rather than appended to).
    api.resetLoopDetector().catch(() => {});

    try {
      let history = chatHistory;

      if (history.length === 0) {
        // Empty history → create a fresh session (which auto-seeds with the
        // system + initial user message via init_agent_conversation).
        const session = await newSession(
          promptToSend,
          explorerState.path,
          explorerState.selected_files,
          modelName
        );
        history = session.messages;
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
            // The read hint is shell-specific, so ask the backend for it
            // rather than hardcoding a PowerShell cmdlet here.
            const { read_list_hint } = await api.getPlatformInfo(fileListPath);
            contextUpdate = `[CONTEXT UPDATE - Files have changed]\n\nWorking directory: ${explorerState.path}\nSelected files: ${explorerState.selected_files.length} files (too many to list inline)\n\nThe complete file list has been written to: ${fileListPath}\n\n${read_list_hint}\n\nPlease use these updated files for any subsequent operations.`;
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

  // Pick which proposal to surface. ask_user_question always wins — if the model
  // is asking for clarification, that's a hard pause regardless of any sibling
  // shell-command proposals.
  function selectProposal(proposals: ToolProposal[]): ToolProposal | null {
    if (proposals.length === 0) return null;
    const question = proposals.find((p) => p.tool_name === TOOLS.ASK_USER_QUESTION);
    return question ?? proposals[0];
  }

  // Step 2: Run Agent Step
  async function runAgentStep(history: Message[], shouldAutoExecute = autoExecute, modelOverride?: string) {
    // Capture the run generation at entry. If a session switch / new-chat /
    // spotlight submit bumps it while we are awaiting the stream, every
    // post-await write below is skipped so we never corrupt the new session.
    const myGen = runGeneration.current;
    const isStale = () => runGeneration.current !== myGen;
    try {
      setStreamingText("");
      // Consume the drop-tools-for-next-request flag (set by a prior Break
      // verdict). The backend uses this to send an empty tool-defs list,
      // forcing the model into a text-only reassessment.
      const dropTools = dropToolsForNextRequest.current;
      dropToolsForNextRequest.current = false;
      const result = await api.runAgentStepStream(
        modelOverride || modelName,
        history,
        (chunk) => {
          // Drop chunks from a superseded run (stale session) or after a Stop.
          if (isStale() || stopRequested.current) return;
          if (chunk.kind === "TextDelta") {
            setStreamingText((prev) => prev + chunk.text);
          }
        },
        dropTools
      );

      // A newer run replaced this one while we were streaming — discard
      // everything silently; the new run owns the UI now.
      if (isStale()) return;

      setStreamingText("");

      if (stopRequested.current) {
        stopRequested.current = false;
        return;
      }

      setChatHistory(result.updated_history);

      // Normalize: CommandProposal (legacy string) → synthetic single ToolProposal.
      let proposals: ToolProposal[] = [];
      if (result.response.type === "ToolProposals") {
        proposals = result.response.content as ToolProposal[];
      } else if (result.response.type === "CommandProposal") {
        proposals = [{
          tool_name: platform.shell_tool_name,
          command: result.response.content as string,
        }];
      }

      const proposal = selectProposal(proposals);

      // Branch 1: ask_user_question — never auto-execute, always surface UI.
      if (proposal && proposal.tool_name === TOOLS.ASK_USER_QUESTION && proposal.question_data) {
        setPendingCommand(null);
        setPendingQuestion({
          data: proposal.question_data,
          tool_call_id: proposal.tool_call_id,
        });
        // Asking for clarification is not a fix-loop — reset the failure counter.
        consecutiveFailures.current = 0;
        if (shouldAutoExecute) setAutoExecute(false);
        return;
      }

      // Branch 2: shell command proposal.
      if (proposal && proposal.command) {
        const cmd = proposal.command;
        // Remember the tool_call_id so dismiss/reject can answer the tool_use.
        pendingCommandToolId.current = proposal.tool_call_id;
        autoStepCount.current++;

        // Backend loop detector verdict. Block/Break pauses auto-execute and
        // surfaces the proposal for explicit approval.
        const verdict = result.loop_verdict;
        const loopBlocked =
          verdict && (verdict.kind === "Block" || verdict.kind === "Break");
        if (loopBlocked) {
          if (shouldAutoExecute) setAutoExecute(false);
          setIsProcessing(false);
          // Break verdict: force the NEXT request to drop tool definitions so
          // the model can't propose another action — only reassess in prose
          // or stop. Block verdict only pauses auto-execute and surfaces.
          if (verdict?.kind === "Break") {
            dropToolsForNextRequest.current = true;
          }
          setChatHistory([
            ...result.updated_history,
            {
              role: "user",
              content: `[Loop detected] ${verdict.reason} — stop, reassess, and either ask a clarifying question with ask_user_question or wait for the user's guidance.`,
              synthetic: true,
            },
          ]);
          setPendingCommand(cmd);
          return;
        }

        // Three ways a command may run without a click, in order of scope:
        //   1. this run was pre-authorised ("All", or a modifier-submit)
        //   2. the persisted policy is "auto"
        //   3. the policy is "read_only" AND the backend proved this command
        //      cannot write anything
        // A `dangerous` verdict overrides all three: recognisably destructive
        // commands are never run unattended, and that is not configurable.
        const risk = await api.classifyCommand(cmd).catch(
          // A classifier failure must not become a free pass.
          () => "mutating" as CommandRisk
        );
        const mode = approvalModeRef.current;
        const permitted =
          shouldAutoExecute || mode === "auto" || (mode === "read_only" && risk === "read_only");
        const mayRunUnattended = permitted && risk !== "dangerous";

        if (mayRunUnattended && autoStepCount.current < MAX_AUTO_STEPS) {
          setIsExecuting(true);
          setIsProcessing(false);
          try {
            const output = await api.executeShellCommand(cmd, explorerState?.path || null);
            // The session may have been switched while the command ran.
            if (isStale()) return;
            const failed = /\[Exit code:\s*-?\d+\s*—\s*Failed\]/.test(output);
            if (failed) {
              consecutiveFailures.current++;
            } else {
              consecutiveFailures.current = 0;
            }

            const newMsg: Message = { role: "tool", content: output };
            const newHistory = [...result.updated_history, newMsg];
            setChatHistory(newHistory);
            setIsExecuting(false);

            // Back-to-back failure pause: catches "everything keeps erroring" loops
            // (different commands, all failing). Hash-detection above catches the
            // identical-command ping-pong loops.
            if (consecutiveFailures.current >= MAX_CONSECUTIVE_FAILURES) {
              consecutiveFailures.current = 0;
              setAutoExecute(false);
              setIsProcessing(false);
              setChatHistory([
                ...newHistory,
                {
                  role: "user",
                  content: `[Auto-execute paused] ${MAX_CONSECUTIVE_FAILURES} commands failed in a row. Stop, re-read the errors above, and either explain what's going wrong or wait for the user's guidance instead of trying more variants.`,
                  synthetic: true,
                },
              ]);
              return;
            }

            setIsProcessing(true);
            // Pass the *session* flag through, not `true`: a read-only
            // auto-run inside a manual session must leave the next command
            // to be judged on its own risk.
            await runAgentStep(newHistory, shouldAutoExecute, modelOverride);
          } catch (e) {
            console.error(`Auto-execute failed: ${e}`);
            setIsExecuting(false);
            setAutoExecute(false);
            setPendingCommand(cmd);
          }
        } else {
          if (shouldAutoExecute) setAutoExecute(false);
          setPendingRisk(risk);
          setPendingCommand(cmd);
        }
        return;
      }

      // Branch 3: plain text response (success summary or chatter).
      setPendingCommand(null);

      if (shouldAutoExecute) {
        const responseText = typeof result.response.content === "string" ? result.response.content : "";
        const taskDone = isTaskComplete(responseText);

        if (!taskDone && autoStepCount.current < MAX_AUTO_STEPS) {
          autoStepCount.current++;
          // Mark the loop as still-busy across the delay window so the
          // idle-detection effect doesn't drain a queued prompt into a
          // SECOND concurrent agent loop.
          autoStepPending.current = true;
          setTimeout(async () => {
            autoStepPending.current = false;
            // Bail if a Stop happened or a newer run superseded this one.
            if (stopRequested.current || isStale()) return;
            const newHistory: Message[] = [
              ...result.updated_history,
              {
                role: "user",
                content:
                  `If the task is complete, reply with exactly 'Task Complete' and stop. Otherwise, continue ONLY if there is genuinely more work to do — do not re-run, re-verify, or rephrase steps that already produced the expected output. CRITICAL: if you claimed to create files, you MUST verify each one exists BEFORE declaring Task Complete.`,
                synthetic: true,
              },
            ];
            setChatHistory(newHistory);
            setIsProcessing(true);
            await runAgentStep(newHistory, true, modelOverride);
          }, AUTO_STEP_DELAY_MS);
          return;
        }
      }

      setAutoExecute(false);
    } catch (e) {
      console.error(`Error running agent step: ${e}`);
      showError(`LLM Error: ${String(e)}`);
      setStreamingText("");
    } finally {
      setIsProcessing(false);
    }
  }

  // Step 2b: User answers a question — feed the choice back as a tool result.
  async function answerQuestion(answer: string) {
    if (!pendingQuestion) return;
    const q = pendingQuestion;
    setPendingQuestion(null);
    const toolResult: Message = {
      role: "tool",
      content: answer,
      tool_calls: q.tool_call_id
        ? [{
            id: q.tool_call_id,
            function: { name: TOOLS.ASK_USER_QUESTION, arguments: {} },
          }]
        : undefined,
    };
    const newHistory = [...chatHistory, toolResult];
    setChatHistory(newHistory);
    setIsProcessing(true);
    await runAgentStep(newHistory);
  }

  function dismissQuestion() {
    if (!pendingQuestion) return;
    const q = pendingQuestion;
    setPendingQuestion(null);
    // Surface the dismissal as a tool result so the model has context for the next turn.
    const toolResult: Message = {
      role: "tool",
      content: "[User dismissed the question without answering.]",
      tool_calls: q.tool_call_id
        ? [{
            id: q.tool_call_id,
            function: { name: TOOLS.ASK_USER_QUESTION, arguments: {} },
          }]
        : undefined,
    };
    setChatHistory((prev) => [...prev, toolResult]);
    setIsProcessing(false);
  }

  async function stopAgent() {
    stopRequested.current = true;
    setAutoExecute(false);

    // Cancel the LLM stream backend-side (tears down the network connection)
    api.cancelStream().catch(() => {});

    const partial = streamingText;
    setStreamingText("");

    // Preserve any partial assistant text + an interrupt note so the model knows
    // it was stopped on the next turn, and the user can pick up the conversation.
    setChatHistory((prev) => {
      const next = [...prev];
      if (partial.trim()) {
        next.push({ role: "assistant", content: partial });
      }
      next.push({
        role: "user",
        content:
          "[Interrupted by user] The user stopped this action. STOP what you are doing and wait for the user to tell you how to proceed.",
        synthetic: true,
      });
      return next;
    });

    setIsProcessing(false);
    setIsExecuting(false);
    setPendingCommand(null);
    setPendingQuestion(null);
    pendingCommandToolId.current = undefined;
    autoStepPending.current = false;
    api.resetLoopDetector().catch(() => {});

    // If a command is still running, ask the user whether to kill it.
    try {
      const running = await api.getRunningCommand();
      if (running) {
        setKillDialog({ pid: running.pid, command: running.command_preview });
      }
    } catch {
      // ignore — not critical
    }
  }

  async function confirmKill() {
    if (!killDialog) return;
    try {
      await api.killRunningCommand();
    } catch (e) {
      showError(`Failed to kill process: ${String(e)}`);
    } finally {
      setKillDialog(null);
    }
  }

  // Build a tool-role message that ANSWERS the pending proposal's tool_use.
  // Every path that consumes a proposal (approve/reject/dismiss) must emit one
  // of these so the assistant's tool_use is never left orphaned — Anthropic
  // rejects a tool_use with no matching tool_result on the next turn (HTTP 400).
  function buildToolResult(content: string): Message {
    const id = pendingCommandToolId.current;
    return {
      role: "tool",
      content,
      tool_calls: id
        ? [{ id, function: { name: platform.shell_tool_name, arguments: {} } }]
        : undefined,
    };
  }

  // Step 3: Approve Command
  async function approveCommand(continueAuto = false) {
    if (!pendingCommand) return;

    if (continueAuto) setAutoExecute(true);

    setIsExecuting(true);
    try {
      const output = await api.executeShellCommand(pendingCommand, explorerState?.path || null);

      const newHistory = [...chatHistory, buildToolResult(output)];
      pendingCommandToolId.current = undefined;
      setChatHistory(newHistory);
      setPendingCommand(null);
      setIsExecuting(false);

      setIsProcessing(true);
      await runAgentStep(newHistory, continueAuto || autoExecute);
    } catch (e) {
      console.error(`Command Execution Failed: ${e}`);
      showError(`Command execution failed: ${String(e)}`);
      setIsExecuting(false);
      setIsProcessing(false);
    }
  }

  function rejectCommand(feedback: string) {
    // Answer the tool_use with the rejection as its result (keeps the
    // assistant tool_use / tool_result pairing valid), then continue.
    const rejection = feedback.trim()
      ? `[User rejected the command] ${feedback.trim()}`
      : "[User rejected the command without additional feedback.]";
    const newHistory: Message[] = [...chatHistory, buildToolResult(rejection)];
    pendingCommandToolId.current = undefined;
    setChatHistory(newHistory);
    setPendingCommand(null);
    setIsProcessing(true);
    runAgentStep(newHistory);
  }

  function dismissCommand() {
    // Record the dismissal as a tool_result so the orphaned tool_use is
    // answered, then stop and wait for the user. Without this, the next turn
    // would send an unanswered tool_use to Anthropic and 400.
    if (pendingCommandToolId.current !== undefined) {
      setChatHistory((prev) => [
        ...prev,
        buildToolResult("[User dismissed the proposed command without running it.]"),
      ]);
      pendingCommandToolId.current = undefined;
    }
    setPendingCommand(null);
    setIsProcessing(false);
  }

  // Stable identity so the memoized ChatMessage rows don't re-render on every
  // streaming token (each token re-renders MainApp; without a stable onToggle
  // the memo boundary would be defeated).
  const toggleExpand = useCallback((idx: number) => {
    setExpandedResponses((prev) => ({ ...prev, [idx]: !prev[idx] }));
  }, []);

  // Refresh on Settings open — pulls latest skills, model list, key state.
  useEffect(() => {
    if (!isSettingsOpen) return;
    Promise.all([
      fetchModels(),
      api
        .listSkills()
        .then((r) => {
          setSkills(r.skills);
          setQuarantinedSkills(r.quarantined);
        })
        .catch(() => {}),
    ]).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isSettingsOpen]);

  const handleToggleShowFreeOpenrouterModels = async () => {
    const next = !showFreeOpenrouterModels;
    try {
      await api.setShowFreeOpenrouterModels(next);
      setShowFreeOpenrouterModels(next);
      fetchModels();
    } catch (e) {
      showError("Failed to toggle free-tier models: " + String(e));
    }
  };

  const handleDismissMigrationBanner = async () => {
    setShowMigrationBanner(false);
    try {
      await api.setOpenrouterDisclosureAck(true);
    } catch {
      // ignore — banner just won't re-suppress on next launch
    }
  };

  const handleOpenSettingsFromBanner = async () => {
    await handleDismissMigrationBanner();
    setIsSettingsOpen(true);
  };

  const handleSettingsSave = async () => {
    try {
      await api.setOllamaUrl(tempOllamaUrl);
      setOllamaUrl(tempOllamaUrl);
      setOllamaConnected(null);
      if (openrouterKey.trim()) await api.setOpenrouterKey(openrouterKey.trim());
      if (anthropicKey.trim()) await api.setAnthropicKey(anthropicKey.trim());
      // Clear the edit-session plaintext so it's not retained in renderer state
      // after save. Refresh the masked preview so the placeholder updates.
      setOpenrouterKey("");
      setAnthropicKey("");
      try {
        const keys = await api.getApiKeys();
        setOpenrouterPreview(keys.openrouter_preview);
        setAnthropicPreview(keys.anthropic_preview);
      } catch {
        // ignore — placeholder will refresh on next Settings open
      }
      setIsSettingsOpen(false);
      fetchModels();
    } catch (e) {
      showError("Failed to save settings: " + String(e));
    }
  };

  // --- Sessions ---

  async function handleNewChat() {
    await flushSave();
    await hardResetSessionUi();
    setChatHistory([]);
    setCurrentSessionId(null);
    lastSyncedContext.current = null;
    setSyncPending(false);
    refreshSessions();
    setTimeout(() => inputRef.current?.focus(), 50);
  }

  async function handleSelectSession(id: string) {
    if (id === currentSessionId) return;
    try {
      await flushSave();
      await hardResetSessionUi();
      const session = await loadSessionFromStore(id);
      setChatHistory(session.messages);
      setModelName(session.model);
      // Synthesize an ExplorerState from the session's saved context so the
      // toolbar reflects what the user was working on. The real file manager may
      // have moved on; the user can press Sync to refresh.
      setExplorerState({
        path: session.explorer_path,
        selected_files: [],
      });
      lastSyncedContext.current = {
        path: session.explorer_path,
        files: [],
      };
      setSyncPending(false);
    } catch (e) {
      showError(`Failed to load session: ${String(e)}`);
    }
  }

  async function handleDeleteSession(id: string) {
    try {
      await deleteSessionFromStore(id);
      if (id === currentSessionId) {
        setChatHistory([]);
        setExplorerState(null);
        await hardResetSessionUi();
      }
    } catch (e) {
      showError(`Failed to delete session: ${String(e)}`);
    }
  }

  async function handleRenameSession(id: string, title: string) {
    try {
      await renameSessionInStore(id, title);
    } catch (e) {
      showError(`Failed to rename session: ${String(e)}`);
    }
  }

  /**
   * Continue affordance — shown when the model stopped on a prose response
   * without emitting a tool_call, code block, OR "Task Complete". Weak local
   * models (Gemma, small Llamas via Ollama) often respond with "I'll use
   * ffmpeg..." and stall there. Clicking Continue appends a synthetic nudge
   * and re-runs the agent.
   */
  async function handleContinueAgent() {
    if (chatHistory.length === 0) return;
    const last = chatHistory[chatHistory.length - 1];
    if (!last || last.role !== "assistant") return;
    const nudge: Message = {
      role: "user",
      content:
        `Continue. You stopped without actually generating the command. Now emit the ${platform.shell_tool_name} tool call directly, OR write the complete ${platform.shell_name} script inside a code block. Do not explain again — produce the code.`,
      synthetic: true,
    };
    const newHistory: Message[] = [...chatHistory, nudge];
    setChatHistory(newHistory);
    setIsProcessing(true);
    await runAgentStep(newHistory);
  }

  // Should we show the "Continue" affordance?
  const shouldShowContinue = (() => {
    if (
      isProcessing ||
      isExecuting ||
      streamingText.length > 0 ||
      pendingCommand !== null ||
      pendingQuestion !== null
    ) {
      return false;
    }
    if (chatHistory.length === 0) return false;
    const last = chatHistory[chatHistory.length - 1];
    if (!last || last.role !== "assistant") return false;
    if (last.tool_calls && last.tool_calls.length > 0) return false;
    const lastContent = last.content ?? "";
    if (isTaskComplete(lastContent)) return false;
    // Don't suggest Continue if the last message looks like a question to
    // the user — the model is waiting on their reply, not stalled.
    if (lastContent.trim().endsWith("?")) return false;
    return true;
  })();

  useEffect(() => {
    if (!isSettingsOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsSettingsOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isSettingsOpen]);

  return (
    <main className="flex flex-col h-screen text-gray-100 font-sans overflow-hidden">
      <TitleBar />
      <div className="flex flex-1 overflow-hidden bg-gray-950">
        <SessionsSidebar
          sessions={sessions}
          currentSessionId={currentSessionId}
          collapsed={sidebarCollapsed}
          onToggleCollapsed={() => setSidebarCollapsed((c) => !c)}
          onNewChat={handleNewChat}
          onSelect={handleSelectSession}
          onDelete={handleDeleteSession}
          onRename={handleRenameSession}
        />
        <div className="flex flex-col flex-1 min-w-0 relative">

      {killDialog && (
        <div className="absolute inset-0 z-50 bg-black/60 backdrop-blur-sm flex items-center justify-center p-6 animate-in fade-in duration-150">
          <div className="bg-gray-900 border border-gray-700 rounded-xl shadow-2xl max-w-md w-full p-6 animate-in zoom-in-95 duration-150">
            <h3 className="text-base font-semibold text-gray-100 mb-2">
              A command is still running
            </h3>
            <p className="text-sm text-gray-400 mb-3">
              The LLM stream was stopped, but a command is still running on your system. Do you want to kill it?
            </p>
            <div className="bg-black/50 border border-gray-800 rounded-md p-3 mb-4 max-h-32 overflow-y-auto">
              <code className="text-xs text-gray-300 break-all whitespace-pre-wrap font-mono">
                {killDialog.command}
              </code>
              <div className="text-[10px] text-gray-600 mt-2">
                PID: {killDialog.pid}
              </div>
            </div>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => setKillDialog(null)}
                className="px-4 py-2 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded-md text-sm font-medium transition"
              >
                Keep running
              </button>
              <button
                onClick={confirmKill}
                className="px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-md text-sm font-medium transition"
              >
                Kill process
              </button>
            </div>
          </div>
        </div>
      )}

      {isSettingsOpen && (
        <SettingsPage
          ollamaUrl={tempOllamaUrl}
          onOllamaUrlChange={setTempOllamaUrl}
          platform={platform}
          onError={showError}
          approvalMode={approvalMode}
          onApprovalModeChange={async (mode) => {
            // Persist immediately rather than on Save: an approval policy
            // that silently reverts because the user closed the panel is a
            // security surprise.
            setApprovalMode(mode);
            try {
              await api.setApprovalMode(mode);
            } catch (e) {
              showError(String(e));
            }
          }}
          openrouterKey={openrouterKey}
          anthropicKey={anthropicKey}
          onOpenrouterKeyChange={setOpenrouterKey}
          onAnthropicKeyChange={setAnthropicKey}
          openrouterPreview={openrouterPreview}
          anthropicPreview={anthropicPreview}
          showFreeOpenrouterModels={showFreeOpenrouterModels}
          onToggleShowFreeOpenrouterModels={handleToggleShowFreeOpenrouterModels}
          skills={skills}
          quarantinedSkills={quarantinedSkills}
          onSave={handleSettingsSave}
          onClose={() => setIsSettingsOpen(false)}
        />
      )}

      {showMigrationBanner && !isSettingsOpen && (
        <MigrationBanner
          onOpenSettings={handleOpenSettingsFromBanner}
          onDismiss={handleDismissMigrationBanner}
          onError={showError}
        />
      )}

      {/* Toolbar */}
      <div className="flex-none h-10 bg-gray-900/50 border-b border-gray-800 flex items-center px-3 gap-2">
        {/* Left: Sync + Context */}
        <div className="flex items-center gap-1.5">
          {autoExecute && (
            // A pre-authorised run has to be visible and revocable. Without
            // this, a Cmd+Enter submit leaves the app executing unattended
            // with nothing on screen saying so.
            <button
              onClick={() => setAutoExecute(false)}
              title="Running unattended — click to require approval again"
              className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium bg-amber-500/20 text-amber-300 border border-amber-500/40 hover:bg-amber-500/30 transition"
            >
              <Zap size={13} />
              <span>Auto</span>
            </button>
          )}
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
              title={explorerState ? `Click to sync with ${platform.file_manager}` : `Sync with ${platform.file_manager}`}
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
              onClick={handleNewChat}
              className="p-1.5 rounded-md text-gray-500 hover:text-gray-300 hover:bg-gray-800/50 transition"
              title="New Chat"
            >
              <X size={16} />
            </button>
          )}

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

      {/* Chat Area — stop button moved into ChatInput so it replaces send
          while busy (Claude.ai pattern). No more overlay covering messages. */}
      <section className="flex-1 flex flex-col relative overflow-hidden">
        {chatHistory.length === 0 ? (
          // Once the first model fetch has resolved (ollamaConnected !== null)
          // and there is still nothing usable, guide the user through setup
          // instead of showing a misleading "Agent Ready".
          ollamaConnected !== null && availableModels.length === 0 ? (
            <ProviderSetup
              status={providerStatus}
              onRecheck={async () => {
                await fetchModels();
                refreshProviderStatus();
              }}
              onSaveKey={async (key) => {
                await api.setOpenrouterKey(key);
                try {
                  const keys = await api.getApiKeys();
                  setOpenrouterPreview(keys.openrouter_preview);
                  setAnthropicPreview(keys.anthropic_preview);
                } catch {
                  // ignore — previews refresh on next Settings open
                }
                await fetchModels();
                refreshProviderStatus();
              }}
              ollamaUrl={ollamaUrl}
              onSaveOllamaUrl={async (url) => {
                await api.setOllamaUrl(url);
                setOllamaUrl(url);
                // Keep the Settings draft in sync so opening Settings next
                // doesn't show the stale URL and save over this one.
                setTempOllamaUrl(url);
                setOllamaConnected(null);
                await fetchModels();
                refreshProviderStatus();
              }}
              onOpenSettings={() => setIsSettingsOpen(true)}
              onError={showError}
            />
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center opacity-30 select-none">
              <Bot size={48} />
              <p className="mt-2 text-sm font-medium">Agent Ready</p>
            </div>
          )
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
              </div>
            )}
            <div ref={chatEndRef} />
          </div>
        )}
      </section>

      {pendingQuestion && (
        <QuestionApproval
          question={pendingQuestion}
          onAnswer={answerQuestion}
          onDismiss={dismissQuestion}
          disabled={isProcessing}
        />
      )}

      {shouldShowContinue && (
        <div className="flex-none px-3 pb-2">
          <div className="max-w-3xl mx-auto flex items-center gap-2 px-3 py-2 bg-amber-900/15 border border-amber-500/30 rounded-md">
            <span className="text-[11px] text-amber-200/80 flex-1 leading-snug">
              The agent stopped without acting. If this isn't a final answer,
              push it to emit the command.
            </span>
            <button
              onClick={handleContinueAgent}
              className="px-3 py-1 bg-amber-600 hover:bg-amber-500 text-white rounded text-xs font-medium transition inline-flex items-center gap-1 shrink-0"
              title="Inject a 'now emit the code' nudge and re-run the agent"
            >
              <RefreshCw size={10} />
              Continue
            </button>
          </div>
        </div>
      )}

      {pendingCommand && (
        <CommandApproval
          command={pendingCommand}
          risk={pendingRisk}
          onChange={setPendingCommand}
          onApprove={approveCommand}
          onReject={rejectCommand}
          onDismiss={dismissCommand}
          isExecuting={isExecuting}
          isProcessing={isProcessing}
        />
      )}

      {queuedPrompts.length > 0 && (
        <div className="flex-none border-t border-gray-800 px-3 py-2 bg-gray-900/30">
          <div className="max-w-3xl mx-auto flex flex-wrap items-center gap-1.5">
            <span className="text-[10px] uppercase tracking-wider text-gray-500 font-semibold mr-1">
              Queued
            </span>
            {queuedPrompts.map((q, i) => (
              <div
                key={i}
                className="flex items-center gap-1.5 bg-indigo-900/30 border border-indigo-500/30 text-indigo-200 text-xs rounded-full pl-3 pr-1 py-0.5 max-w-xs"
                title={q}
              >
                <span className="truncate">{q}</span>
                <button
                  onClick={() => removeQueuedPrompt(i)}
                  className="w-4 h-4 flex items-center justify-center rounded-full hover:bg-indigo-700/40 text-indigo-300"
                  title="Remove from queue"
                >
                  <X size={10} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      <ChatInput
        ref={inputRef}
        prompt={agentPrompt}
        onChange={setAgentPrompt}
        onSubmit={submitOrQueue}
        disabled={false}
        hasPendingCommand={pendingCommand !== null}
        modelName={modelName}
        onModelChange={setModelName}
        models={availableModels}
        ollamaConnected={ollamaConnected}
        isBusy={
          (isProcessing || isExecuting || streamingText.length > 0) &&
          !pendingCommand &&
          !pendingQuestion
        }
        onStop={stopAgent}
        onRefreshModels={refreshModels}
        isRefreshingModels={isRefreshingModels}
      />

        </div>
      </div>

      <ErrorToast message={error} onDismiss={clearError} />
    </main>
  );
}

export default App;
