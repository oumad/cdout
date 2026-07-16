import { useState } from "react";
import { ExternalLink, Trash2, X } from "lucide-react";
import * as api from "../utils/tauri";

interface MigrationBannerProps {
  onOpenSettings: () => void;
  onDismiss: () => void;
  onError?: (msg: string) => void;
}

/**
 * One-time onboarding banner shown after the OpenRouter migration when cdout
 * detects legacy credentials (Claude Code OAuth, Codex CLI, Antigravity, or
 * old openai/gemini api_keys.json fields) on disk. Pointed at openrouter.ai
 * because the legacy auth paths are gone — the user has to pick a new home for
 * their cloud requests.
 */
export function MigrationBanner({
  onOpenSettings,
  onDismiss,
  onError,
}: MigrationBannerProps) {
  const [cleaning, setCleaning] = useState(false);

  async function handleCleanup() {
    const removeThirdParty = confirm(
      "Also delete ~/.claude/.credentials.json and ~/.codex/auth.json? " +
      "These files belong to other tools (Claude Code, Codex CLI). " +
      "Only remove them if you no longer use those CLIs. " +
      "\n\nOK = delete cdout artefacts + third-party CLI creds. " +
      "Cancel = only delete cdout's own antigravity_credentials.json."
    );
    setCleaning(true);
    try {
      await api.cleanupLegacyCredentials(removeThirdParty);
      onDismiss();
    } catch (e) {
      onError?.(String(e));
    } finally {
      setCleaning(false);
    }
  }
  return (
    <div className="flex-none border-b border-amber-500/30 bg-amber-900/15 px-4 py-3">
      <div className="max-w-3xl mx-auto flex items-start gap-3">
        <div className="flex-1 min-w-0">
          <div className="text-sm font-semibold text-amber-200 mb-1">
            Cloud providers changed
          </div>
          <p className="text-xs text-amber-100/80 leading-relaxed">
            cdout now routes cloud LLM calls through{" "}
            <a
              href="https://openrouter.ai"
              target="_blank"
              rel="noopener noreferrer"
              className="text-amber-200 underline inline-flex items-center gap-1"
            >
              OpenRouter <ExternalLink size={10} />
            </a>{" "}
            instead of CLI-credential reuse. The Claude Code OAuth path, Codex
            CLI integration, Gemini CLI detection, and Antigravity provider
            have all been removed. Get an OpenRouter API key (one key, ~400
            models including Claude Opus 4.7, GPT-5, Gemini 3) and paste it
            into Settings → Cloud. Your Ollama models still work as-is.
          </p>
          <p className="text-[11px] text-amber-100/60 leading-relaxed mt-2">
            Your old credential files were left on disk untouched. You can
            clean them up below or leave them — cdout no longer reads
            them either way.
          </p>
          <div className="mt-3 flex items-center gap-2 flex-wrap">
            <button
              onClick={onOpenSettings}
              className="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white rounded-md text-xs font-medium transition"
            >
              Open Settings
            </button>
            <a
              href="https://openrouter.ai/keys"
              target="_blank"
              rel="noopener noreferrer"
              className="px-3 py-1.5 bg-amber-900/40 hover:bg-amber-800/50 border border-amber-500/40 text-amber-200 rounded-md text-xs font-medium transition inline-flex items-center gap-1"
            >
              Get an OpenRouter key <ExternalLink size={10} />
            </a>
            <button
              onClick={handleCleanup}
              disabled={cleaning}
              className="px-3 py-1.5 bg-red-900/40 hover:bg-red-800/50 border border-red-500/40 text-red-200 rounded-md text-xs font-medium transition inline-flex items-center gap-1 disabled:opacity-50"
              title="Delete legacy credential files from disk"
            >
              <Trash2 size={10} />
              {cleaning ? "Cleaning…" : "Clean up legacy creds"}
            </button>
          </div>
        </div>
        <button
          onClick={onDismiss}
          className="p-1 rounded text-amber-400/70 hover:text-amber-200 hover:bg-amber-900/40 shrink-0"
          title="Dismiss"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
