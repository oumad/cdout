import { useState } from "react";
import { HelpCircle, Check, X } from "lucide-react";
import type { PendingQuestion } from "../types";

interface QuestionApprovalProps {
  question: PendingQuestion;
  onAnswer: (answer: string) => void;
  onDismiss: () => void;
  disabled?: boolean;
}

export function QuestionApproval({
  question,
  onAnswer,
  onDismiss,
  disabled,
}: QuestionApprovalProps) {
  const { data } = question;
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [customAnswer, setCustomAnswer] = useState("");
  const [isCustomOpen, setIsCustomOpen] = useState(false);

  function togglePick(idx: number) {
    setSelected((prev) => {
      const next = new Set(data.multi_select ? prev : []);
      if (prev.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }

  function submit() {
    if (isCustomOpen && customAnswer.trim()) {
      onAnswer(customAnswer.trim());
      return;
    }
    const picked = [...selected].map((i) => data.options[i].label);
    if (picked.length === 0) return;
    onAnswer(picked.join(", "));
  }

  const canSubmit =
    (isCustomOpen && customAnswer.trim().length > 0) ||
    (!isCustomOpen && selected.size > 0);

  return (
    <div className="flex-none p-3 pt-0">
      <div className="bg-black/40 border border-indigo-500/30 rounded-lg overflow-hidden">
        <div className="bg-indigo-500/10 px-3 py-1.5 flex items-center justify-between border-b border-indigo-500/20">
          <div className="flex items-center gap-2 text-indigo-300 text-[10px] font-bold uppercase tracking-wider">
            <HelpCircle size={12} />
            {data.header ? data.header : "Quick question"}
            {data.multi_select && (
              <span className="text-[10px] text-indigo-400/70 normal-case tracking-normal">
                (pick one or more)
              </span>
            )}
          </div>
        </div>
        <div className="p-3 space-y-3">
          <div className="text-sm text-gray-200 leading-relaxed">
            {data.question}
          </div>

          <div className="space-y-1.5">
            {data.options.map((opt, i) => {
              const isPicked = selected.has(i);
              return (
                <button
                  key={i}
                  type="button"
                  disabled={disabled || isCustomOpen}
                  onClick={() => togglePick(i)}
                  className={`w-full text-left px-3 py-2 rounded-md border text-sm transition disabled:opacity-50 ${
                    isPicked
                      ? "bg-indigo-600/20 border-indigo-500/60 text-indigo-100"
                      : "bg-gray-900/60 border-gray-700/60 text-gray-300 hover:bg-gray-800/80 hover:border-gray-600"
                  }`}
                >
                  <div className="flex items-start gap-2">
                    <div
                      className={`mt-0.5 w-4 h-4 rounded-${
                        data.multi_select ? "sm" : "full"
                      } border-2 flex items-center justify-center shrink-0 ${
                        isPicked
                          ? "border-indigo-400 bg-indigo-500"
                          : "border-gray-600"
                      }`}
                    >
                      {isPicked && (
                        <Check size={10} className="text-white" strokeWidth={3} />
                      )}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="font-medium">{opt.label}</div>
                      {opt.description && (
                        <div className="text-xs text-gray-400 mt-0.5">
                          {opt.description}
                        </div>
                      )}
                    </div>
                  </div>
                </button>
              );
            })}

            <button
              type="button"
              disabled={disabled}
              onClick={() => {
                setIsCustomOpen((v) => !v);
                if (!isCustomOpen) setSelected(new Set());
              }}
              className={`w-full text-left px-3 py-2 rounded-md border text-sm transition disabled:opacity-50 ${
                isCustomOpen
                  ? "bg-amber-500/15 border-amber-500/50 text-amber-100"
                  : "bg-gray-900/40 border-gray-800 text-gray-500 hover:text-gray-300 hover:border-gray-700"
              }`}
            >
              Other (type your own answer)
            </button>

            {isCustomOpen && (
              <textarea
                className="w-full bg-amber-950/20 text-amber-100 placeholder-amber-700/50 text-sm p-2 rounded border border-amber-500/20 focus:outline-none focus:border-amber-500/50 resize-none font-sans"
                placeholder="Type your answer..."
                rows={2}
                value={customAnswer}
                onChange={(e) => setCustomAnswer(e.target.value)}
                autoFocus
              />
            )}
          </div>

          <div className="flex gap-2 justify-end pt-1">
            <button
              type="button"
              onClick={onDismiss}
              disabled={disabled}
              className="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded text-xs font-medium transition disabled:opacity-50 flex items-center gap-1.5"
            >
              <X size={12} />
              Dismiss
            </button>
            <button
              type="button"
              onClick={submit}
              disabled={disabled || !canSubmit}
              className="px-4 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded text-xs font-medium transition disabled:opacity-40 flex items-center gap-1.5"
            >
              <Check size={12} />
              Send Answer
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
