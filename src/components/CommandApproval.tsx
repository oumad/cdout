import { useState } from "react";
import { Terminal, Play, Info, X, Loader2, ShieldAlert, Pencil } from "lucide-react";
import type { CommandRisk } from "../types";

interface CommandApprovalProps {
  command: string;
  /**
   * Why this proposal stopped for approval. `null` when unclassified.
   * `read_only` here means the policy is "ask for everything" — the command
   * itself is harmless, which is worth saying so the user can approve it
   * without reading it closely.
   */
  risk?: CommandRisk | null;
  onChange: (command: string) => void;
  onApprove: (continueAuto: boolean) => void;
  onReject: (feedback: string) => void;
  onDismiss: () => void;
  isExecuting: boolean;
  isProcessing: boolean;
}

export function CommandApproval({
  command,
  risk,
  onChange,
  onApprove,
  onReject,
  onDismiss,
  isExecuting,
  isProcessing,
}: CommandApprovalProps) {
  const [isFeedbackOpen, setIsFeedbackOpen] = useState(false);
  const [feedbackText, setFeedbackText] = useState("");

  const disabled = isProcessing || isExecuting;

  return (
    <div className="flex-none p-3 pt-0">
      <div
        className={`bg-black/40 border rounded-lg overflow-hidden ${
          risk === "dangerous" ? "border-red-500/40" : "border-emerald-500/30"
        }`}
      >
        <div
          className={`px-3 py-1.5 flex items-center justify-between border-b ${
            risk === "dangerous"
              ? "bg-red-500/10 border-red-500/20"
              : "bg-emerald-500/10 border-emerald-500/20"
          }`}
        >
          <div
            className={`flex items-center gap-2 text-[10px] font-bold uppercase tracking-wider ${
              risk === "dangerous" ? "text-red-300" : "text-emerald-400"
            }`}
          >
            <Terminal size={12} />
            Command Proposal (editable)
          </div>
          {risk && <RiskBadge risk={risk} />}
        </div>
        <div className="p-3">
          <textarea
            className="block w-full font-mono text-xs text-gray-300 bg-black/50 p-2 rounded border border-white/5 resize-none focus:outline-none focus:border-emerald-500/50 custom-scrollbar"
            rows={3}
            value={command}
            onChange={(e) => onChange(e.target.value)}
            disabled={isExecuting}
          />
          <div className="flex gap-2 mt-3">
            <button
              onClick={() => onApprove(false)}
              disabled={disabled || isFeedbackOpen}
              className="flex-1 py-1.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-xs font-medium transition flex items-center justify-center gap-1.5 disabled:opacity-50"
            >
              {isExecuting ? (
                <Loader2 className="animate-spin" size={12} />
              ) : (
                <Play size={12} />
              )}
              {isExecuting ? "Running..." : "Execute"}
            </button>
            <button
              onClick={() => onApprove(true)}
              disabled={disabled || isFeedbackOpen}
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
                  onReject(feedbackText);
                  setIsFeedbackOpen(false);
                  setFeedbackText("");
                } else {
                  setIsFeedbackOpen(!isFeedbackOpen);
                }
              }}
              disabled={disabled}
              className={`px-4 py-1.5 rounded text-xs font-medium transition flex items-center gap-1.5 disabled:opacity-50 ${
                isFeedbackOpen
                  ? "bg-amber-500 text-white hover:bg-amber-400"
                  : "bg-amber-600/20 text-amber-500 border border-amber-600/30 hover:bg-amber-600/30"
              }`}
            >
              <Info size={12} />
              {isFeedbackOpen ? "Send Feedback" : "Reject with Feedback"}
            </button>
            <button
              onClick={onDismiss}
              disabled={disabled}
              className="px-4 py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded text-xs font-medium transition flex items-center gap-1.5 disabled:opacity-50"
            >
              <X size={12} />
              Dismiss
            </button>
          </div>

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
  );
}

/**
 * Names the reason a proposal needs a look. The destructive case is the one
 * that matters: it is shown even when the user has opted into running
 * everything unattended, because that guard is not disableable.
 */
function RiskBadge({ risk }: { risk: CommandRisk }) {
  if (risk === "dangerous") {
    return (
      <span className="flex items-center gap-1 text-[10px] font-bold uppercase tracking-wider text-red-300">
        <ShieldAlert size={11} />
        Destructive — read it
      </span>
    );
  }
  if (risk === "mutating") {
    return (
      <span className="flex items-center gap-1 text-[10px] font-medium uppercase tracking-wider text-amber-400/80">
        <Pencil size={10} />
        Writes files
      </span>
    );
  }
  return (
    <span className="text-[10px] font-medium uppercase tracking-wider text-gray-500">
      Read-only
    </span>
  );
}
