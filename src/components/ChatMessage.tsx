import { memo } from "react";
import ReactMarkdown from "react-markdown";
import {
  Terminal,
  Info,
  Bot,
  User,
  ChevronDown,
  ChevronUp,
  Settings as SettingsIcon,
} from "lucide-react";
import type { Message } from "../types";
import {
  TOOL_OUTPUT_COLLAPSE_CHARS,
  MESSAGE_COLLAPSE_CHARS,
} from "../constants";

interface ChatMessageProps {
  message: Message;
  index: number;
  isExpanded: boolean;
  onToggle: (index: number) => void;
}

/**
 * Internal nudge messages (auto-step continuations, loop-detected notices,
 * interrupt confirmations) are stored with role="user" so the LLM API accepts
 * them, but should render distinctly so the user never confuses them with
 * their own input. Compact, gray, with a "cdout internal" badge.
 */
function SyntheticNote({ content }: { content: string }) {
  return (
    <div className="flex justify-center my-2 group">
      <div className="bg-gray-900/50 border border-gray-800/60 rounded-md text-[10px] max-w-2xl w-full px-3 py-2 flex items-start gap-2">
        <SettingsIcon
          size={11}
          className="text-gray-600 mt-0.5 shrink-0"
        />
        <div className="flex-1 min-w-0">
          <div className="text-[9px] uppercase tracking-wider text-gray-600 font-semibold mb-0.5">
            cdout internal
          </div>
          <div className="text-gray-500 italic leading-snug whitespace-pre-wrap break-words">
            {content}
          </div>
        </div>
      </div>
    </div>
  );
}

function ChatMessageImpl({
  message: msg,
  index: idx,
  isExpanded,
  onToggle,
}: ChatMessageProps) {
  // Normalize once so a null/undefined content (which TS types as string but a
  // malformed backend payload could still deliver) never throws in
  // ReactMarkdown or a `.length` check downstream.
  const content = msg.content ?? "";

  // Synthetic messages: cdout-injected nudges that travel as role="user" but
  // are NOT user input. Render with the dedicated SyntheticNote shape.
  if (msg.synthetic) {
    return <SyntheticNote content={content} />;
  }

  if (msg.role === "system") {
    return (
      <div className="flex justify-center my-4 group">
        <details className="bg-gray-900 border border-gray-800 rounded-lg text-xs max-w-2xl w-full open:bg-gray-900/50 transition-all">
          <summary className="px-4 py-2 cursor-pointer text-gray-500 hover:text-gray-300 font-medium select-none flex items-center gap-2 list-none">
            <Info size={14} className="opacity-70" />
            <span>System Context</span>
            <span className="ml-auto text-[10px] opacity-40">
              Click to expand
            </span>
          </summary>
          <div className="p-4 pt-0 text-gray-400 font-mono whitespace-pre-wrap border-t border-gray-800/50 mt-2">
            {content}
          </div>
        </details>
      </div>
    );
  }

  return (
    <div
      className={`flex gap-3 ${msg.role === "user" ? "flex-row-reverse" : ""} group`}
    >
      <div
        className={`w-6 h-6 rounded flex items-center justify-center shrink-0 mt-0.5 ${
          msg.role === "user"
            ? "bg-indigo-600 text-white"
            : msg.role === "tool"
              ? "bg-gray-800 text-emerald-500"
              : "bg-emerald-600 text-white"
        }`}
      >
        {msg.role === "user" ? (
          <User size={12} />
        ) : msg.role === "tool" ? (
          <Terminal size={12} />
        ) : (
          <Bot size={12} />
        )}
      </div>

      <div
        className={`max-w-[85%] rounded-lg px-3 py-2 text-sm leading-relaxed shadow-sm relative ${
          msg.role === "user"
            ? "bg-[#1e1e2e] text-indigo-100 border border-indigo-500/10"
            : msg.role === "tool"
              ? "bg-black/40 text-green-400 font-mono border border-gray-800 text-xs w-full overflow-hidden"
              : "text-gray-300"
        }`}
      >
        {msg.role === "tool" ? (
          <div className="flex flex-col">
            <div
              className={`whitespace-pre-wrap ${!isExpanded ? "max-h-32 overflow-hidden relative" : ""}`}
            >
              {content}
              {!isExpanded &&
                content.length > TOOL_OUTPUT_COLLAPSE_CHARS && (
                  <div className="absolute bottom-0 left-0 right-0 h-8 bg-gradient-to-t from-gray-950/80 to-transparent pointer-events-none" />
                )}
            </div>
            {content.length > TOOL_OUTPUT_COLLAPSE_CHARS && (
              <button
                onClick={() => onToggle(idx)}
                className="mt-2 flex items-center gap-1 text-[10px] text-gray-500 hover:text-gray-300 uppercase font-bold tracking-wider"
              >
                {isExpanded ? (
                  <>
                    <ChevronUp size={10} /> Show Less
                  </>
                ) : (
                  <>
                    <ChevronDown size={10} /> Show More
                  </>
                )}
              </button>
            )}
          </div>
        ) : (
          <div className="flex flex-col">
            <div
              className={`prose prose-invert prose-xs max-w-none
                prose-p:leading-snug prose-p:my-1 prose-headings:my-2 prose-li:my-0.5
                prose-pre:bg-black/50 prose-pre:border prose-pre:border-white/10 prose-pre:text-[11px] prose-pre:p-2
                prose-code:text-indigo-300 prose-code:bg-white/5 prose-code:px-1 prose-code:rounded prose-code:before:content-none prose-code:after:content-none
                ${!isExpanded && content.length > MESSAGE_COLLAPSE_CHARS ? "max-h-48 overflow-hidden relative" : ""}`}
            >
              <ReactMarkdown>{content}</ReactMarkdown>
              {!isExpanded && content.length > MESSAGE_COLLAPSE_CHARS && (
                <div className="absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-gray-950 to-transparent pointer-events-none" />
              )}
            </div>
            {content.length > MESSAGE_COLLAPSE_CHARS && (
              <button
                onClick={() => onToggle(idx)}
                className="mt-2 flex items-center gap-1 text-[10px] text-gray-500 hover:text-gray-300 uppercase font-bold tracking-wider self-start"
              >
                {isExpanded ? (
                  <>
                    <ChevronUp size={10} /> Show Less
                  </>
                ) : (
                  <>
                    <ChevronDown size={10} /> Show More
                  </>
                )}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Memoized so a streaming token (which re-renders the parent ~80x/sec) does
 * NOT re-run ReactMarkdown for every historical message — only the streaming
 * bubble updates. `onToggle` must be a stable reference (useCallback in the
 * parent) for this memo to be effective.
 */
export const ChatMessage = memo(ChatMessageImpl);
