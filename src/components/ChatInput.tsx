import { forwardRef } from "react";
import { ArrowUp, Square, RefreshCw } from "lucide-react";
import { ModelSelector } from "./ModelSelector";

interface ChatInputProps {
  prompt: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  disabled: boolean;
  hasPendingCommand: boolean;
  modelName: string;
  onModelChange: (model: string) => void;
  models: string[];
  ollamaConnected: boolean | null;
  /**
   * When true, the send button is replaced by a Stop button which calls
   * `onStop`. Matches the Claude.ai / ChatGPT pattern and stops the Stop
   * overlay from covering the chat area.
   */
  isBusy?: boolean;
  onStop?: () => void;
  /** When provided, shows a small "refresh model list" icon next to the picker. */
  onRefreshModels?: () => void;
  isRefreshingModels?: boolean;
}

export const ChatInput = forwardRef<HTMLInputElement, ChatInputProps>(
  function ChatInput(
    {
      prompt,
      onChange,
      onSubmit,
      disabled,
      hasPendingCommand,
      modelName,
      onModelChange,
      models,
      ollamaConnected,
      isBusy = false,
      onStop,
      onRefreshModels,
      isRefreshingModels = false,
    },
    ref
  ) {
    return (
      <div className="flex-none border-t border-gray-800 p-3 bg-gray-900/50">
        <div className="max-w-3xl mx-auto bg-gray-900 rounded-xl border border-gray-800 overflow-hidden focus-within:border-gray-700 transition">
          <input
            ref={ref}
            className="w-full bg-transparent text-sm text-gray-200 placeholder-gray-600 px-4 pt-3 pb-2 focus:outline-none"
            placeholder={
              hasPendingCommand
                ? "Approve command above..."
                : isBusy
                  ? "Agent is working… you can type while it runs"
                  : "Ask me anything..."
            }
            value={prompt}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSubmit();
            }}
            disabled={disabled}
            autoFocus
          />
          <div className="flex items-center justify-end gap-2 px-3 pb-2 min-w-0">
            <div className="flex items-center gap-1 min-w-0 max-w-[60%]">
              <ModelSelector
                modelName={modelName}
                onChange={onModelChange}
                models={models}
                connected={ollamaConnected}
                disabled={disabled}
                maxDisplayChars={32}
                className={`text-[11px] rounded px-1 py-0.5 truncate min-w-0 ${
                  ollamaConnected === false
                    ? "text-red-400"
                    : models.length === 0
                      ? "text-amber-400"
                      : "text-gray-500 hover:text-gray-400"
                }`}
              />
              {onRefreshModels && (
                <button
                  type="button"
                  onClick={onRefreshModels}
                  disabled={isRefreshingModels}
                  className="p-1 rounded text-gray-600 hover:text-gray-300 hover:bg-gray-800/60 transition disabled:opacity-40 shrink-0"
                  title="Refresh model list"
                >
                  <RefreshCw
                    size={10}
                    className={isRefreshingModels ? "animate-spin" : ""}
                  />
                </button>
              )}
            </div>
            {isBusy && onStop ? (
              <button
                type="button"
                onClick={onStop}
                className="w-7 h-7 flex items-center justify-center rounded-lg bg-red-600 hover:bg-red-500 text-white transition animate-in zoom-in-95 duration-150"
                title="Stop"
              >
                <Square size={12} strokeWidth={2.5} fill="white" />
              </button>
            ) : (
              <button
                type="button"
                onClick={onSubmit}
                disabled={disabled || !prompt.trim()}
                className="w-7 h-7 flex items-center justify-center rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white transition disabled:opacity-30 disabled:hover:bg-indigo-600"
                title="Send"
              >
                <ArrowUp size={16} strokeWidth={2.5} />
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }
);
