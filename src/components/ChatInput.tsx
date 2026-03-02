import { forwardRef } from "react";
import { ArrowUp } from "lucide-react";
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
          <div className="flex items-center justify-end gap-2 px-3 pb-2">
            <ModelSelector
              modelName={modelName}
              onChange={onModelChange}
              models={models}
              connected={ollamaConnected}
              disabled={disabled}
              className={`text-[11px] rounded px-1 py-0.5 ${
                ollamaConnected === false
                  ? "text-red-400"
                  : models.length === 0
                    ? "text-amber-400"
                    : "text-gray-500 hover:text-gray-400"
              }`}
            />
            <button
              onClick={onSubmit}
              disabled={disabled || !prompt.trim()}
              className="w-7 h-7 flex items-center justify-center rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white transition disabled:opacity-30 disabled:hover:bg-indigo-600"
              title="Send"
            >
              <ArrowUp size={16} strokeWidth={2.5} />
            </button>
          </div>
        </div>
      </div>
    );
  }
);
