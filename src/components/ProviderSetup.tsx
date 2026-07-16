import { useState } from "react";
import {
  Bot,
  Cpu,
  Cloud,
  ExternalLink,
  RefreshCw,
  Loader2,
  Check,
  Settings as SettingsIcon,
} from "lucide-react";
import type { ProviderStatus } from "../types";

interface ProviderSetupProps {
  status: ProviderStatus | null;
  /** Re-check Ollama + keys and refresh the model list. */
  onRecheck: () => void | Promise<void>;
  /** Persist an OpenRouter key, then refresh models. */
  onSaveKey: (key: string) => Promise<void>;
  onOpenSettings: () => void;
  onError?: (msg: string) => void;
}

/**
 * First-run onboarding. Shown in the empty chat state when no model is usable
 * yet (no cloud key, and either Ollama isn't running or has no pulled models).
 * Offers the two real paths side by side and unmounts itself the moment a
 * model becomes available — the caller gates rendering on the model list, and
 * both actions here refresh it.
 */
export function ProviderSetup({
  status,
  onRecheck,
  onSaveKey,
  onOpenSettings,
  onError,
}: ProviderSetupProps) {
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [rechecking, setRechecking] = useState(false);

  async function handleSave() {
    const trimmed = key.trim();
    if (!trimmed) return;
    setSaving(true);
    try {
      await onSaveKey(trimmed);
      setKey("");
    } catch (e) {
      onError?.(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleRecheck() {
    setRechecking(true);
    try {
      await onRecheck();
    } finally {
      setRechecking(false);
    }
  }

  // Ollama status line — the whole point of the honest reachability signal.
  const ollamaReachable = status?.ollama_reachable ?? false;

  return (
    <div className="flex-1 overflow-y-auto custom-scrollbar px-6 py-10">
      <div className="max-w-3xl mx-auto">
        <div className="flex flex-col items-center text-center mb-8">
          <div className="w-12 h-12 rounded-xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center mb-4">
            <Bot size={24} className="text-indigo-400" />
          </div>
          <h2 className="text-lg font-semibold text-gray-100">
            Set up a model to get started
          </h2>
          <p className="text-sm text-gray-400 mt-1.5 max-w-md">
            cdout needs a model to think. Pick either one — both work, and you
            can switch anytime from the picker.
          </p>
        </div>

        <div className="grid md:grid-cols-2 gap-4">
          {/* ── Local (Ollama) ── */}
          <div className="bg-gray-900 border border-gray-800 rounded-xl p-5 flex flex-col">
            <div className="flex items-center gap-2.5 mb-1.5">
              <div className="w-8 h-8 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center">
                <Cpu size={16} className="text-emerald-400" />
              </div>
              <div>
                <div className="text-sm font-semibold text-gray-100">Local</div>
                <div className="text-[10px] uppercase tracking-wider text-emerald-500/80 font-semibold">
                  private · free
                </div>
              </div>
            </div>
            <p className="text-xs text-gray-400 leading-relaxed mb-3">
              Run models on your own machine with Ollama. Prompts never leave
              your computer.
            </p>

            <div className="bg-black/30 border border-gray-800 rounded-lg p-3 text-xs mb-3">
              {status === null ? (
                <span className="text-gray-500 inline-flex items-center gap-1.5">
                  <Loader2 size={12} className="animate-spin" /> Checking for
                  Ollama…
                </span>
              ) : ollamaReachable ? (
                <div className="flex flex-col gap-1.5">
                  <span className="text-emerald-400 inline-flex items-center gap-1.5 font-medium">
                    <Check size={13} /> Ollama is running
                  </span>
                  <span className="text-gray-400">
                    No models yet — pull one, then re-check:
                  </span>
                  <code className="block bg-black/50 border border-gray-800 rounded px-2 py-1.5 text-emerald-300 font-mono text-[11px] select-all">
                    ollama pull llama3.2
                  </code>
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  <span className="text-amber-400 font-medium">
                    Ollama not detected
                  </span>
                  <span className="text-gray-400">
                    Install it, start the app, then re-check.
                  </span>
                </div>
              )}
            </div>

            <div className="mt-auto flex items-center gap-2">
              <button
                onClick={handleRecheck}
                disabled={rechecking}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-gray-800 hover:bg-gray-700 border border-gray-700 text-gray-200 rounded-md text-xs font-medium transition disabled:opacity-50"
              >
                <RefreshCw
                  size={12}
                  className={rechecking ? "animate-spin" : ""}
                />
                Re-check
              </button>
              {!ollamaReachable && (
                <a
                  href="https://ollama.com/download"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 text-xs text-emerald-400 hover:text-emerald-300"
                >
                  Get Ollama <ExternalLink size={10} />
                </a>
              )}
            </div>
          </div>

          {/* ── Cloud (OpenRouter) ── */}
          <div className="bg-gray-900 border border-gray-800 rounded-xl p-5 flex flex-col">
            <div className="flex items-center gap-2.5 mb-1.5">
              <div className="w-8 h-8 rounded-lg bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center">
                <Cloud size={16} className="text-indigo-400" />
              </div>
              <div>
                <div className="text-sm font-semibold text-gray-100">Cloud</div>
                <div className="text-[10px] uppercase tracking-wider text-indigo-400/80 font-semibold">
                  ~400 models
                </div>
              </div>
            </div>
            <p className="text-xs text-gray-400 leading-relaxed mb-3">
              One OpenRouter key unlocks Claude, GPT, Gemini, and more. Paste it
              to start right away.
            </p>

            <div className="flex flex-col gap-2 mb-1">
              <input
                type="password"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSave();
                }}
                placeholder="sk-or-v1-..."
                aria-label="OpenRouter API key"
                className="w-full bg-black/40 border border-gray-700 rounded-md px-3 py-2 text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-indigo-500"
              />
              <button
                onClick={handleSave}
                disabled={saving || !key.trim()}
                className="inline-flex items-center justify-center gap-1.5 px-3 py-2 bg-indigo-600 hover:bg-indigo-500 text-white rounded-md text-xs font-medium transition disabled:opacity-50"
              >
                {saving ? (
                  <Loader2 size={13} className="animate-spin" />
                ) : (
                  <Check size={13} />
                )}
                {saving ? "Saving…" : "Save key & start"}
              </button>
            </div>

            <a
              href="https://openrouter.ai/keys"
              target="_blank"
              rel="noopener noreferrer"
              className="mt-auto inline-flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 pt-2"
            >
              Get a key <ExternalLink size={10} />
            </a>
          </div>
        </div>

        <div className="text-center mt-6">
          <button
            onClick={onOpenSettings}
            className="inline-flex items-center gap-1.5 text-xs text-gray-500 hover:text-gray-300 transition"
          >
            <SettingsIcon size={12} />
            More options (Anthropic direct, free models) in Settings
          </button>
        </div>
      </div>
    </div>
  );
}
