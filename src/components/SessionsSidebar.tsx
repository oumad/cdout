import { useState } from "react";
import {
  MessageSquarePlus,
  PanelLeftClose,
  PanelLeftOpen,
  X,
  Folder,
  Pencil,
} from "lucide-react";
import type { SessionMeta } from "../types";

interface SessionsSidebarProps {
  sessions: SessionMeta[];
  currentSessionId: string | null;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  onNewChat: () => void;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, title: string) => void;
}

function relativeTime(ts: number): string {
  const ms = Date.now() - ts;
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  const months = Math.floor(d / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(months / 12)}y ago`;
}

export function SessionsSidebar({
  sessions,
  currentSessionId,
  collapsed,
  onToggleCollapsed,
  onNewChat,
  onSelect,
  onDelete,
  onRename,
}: SessionsSidebarProps) {
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");

  function commitRename(id: string) {
    const next = renameDraft.trim();
    if (next.length > 0) onRename(id, next);
    setRenamingId(null);
  }

  if (collapsed) {
    return (
      <aside className="w-12 flex-none bg-gray-900/50 border-r border-gray-800 flex flex-col items-center py-2 gap-1">
        <button
          onClick={onToggleCollapsed}
          className="p-2 rounded-md text-gray-500 hover:text-gray-200 hover:bg-gray-800/60 transition"
          title="Expand sidebar"
        >
          <PanelLeftOpen size={16} />
        </button>
        <button
          onClick={onNewChat}
          className="p-2 rounded-md text-indigo-400 hover:text-indigo-300 hover:bg-gray-800/60 transition"
          title="New chat"
        >
          <MessageSquarePlus size={16} />
        </button>
      </aside>
    );
  }

  return (
    <aside className="w-60 flex-none bg-gray-900/50 border-r border-gray-800 flex flex-col">
      <div className="px-3 py-2 border-b border-gray-800 flex items-center justify-between gap-2">
        <button
          onClick={onNewChat}
          className="flex-1 flex items-center gap-2 px-3 py-1.5 bg-indigo-600/20 hover:bg-indigo-600/30 border border-indigo-500/30 text-indigo-200 rounded-md text-xs font-medium transition"
          title="Start a new chat"
        >
          <MessageSquarePlus size={14} />
          <span>New chat</span>
        </button>
        <button
          onClick={onToggleCollapsed}
          className="p-1.5 rounded-md text-gray-500 hover:text-gray-200 hover:bg-gray-800/60 transition"
          title="Collapse sidebar"
        >
          <PanelLeftClose size={14} />
        </button>
      </div>

      <nav className="flex-1 overflow-y-auto custom-scrollbar py-1">
        {sessions.length === 0 ? (
          <div className="px-4 py-6 text-xs text-gray-600 leading-relaxed">
            No sessions yet. Trigger the spotlight hotkey to start one.
          </div>
        ) : (
          sessions.map((s) => {
            const isActive = s.id === currentSessionId;
            const isRenaming = renamingId === s.id;
            return (
              <div
                key={s.id}
                className={`group relative mx-1 my-0.5 rounded-md px-2.5 py-1.5 cursor-pointer text-xs transition border ${
                  isActive
                    ? "bg-indigo-600/20 border-indigo-500/40 text-indigo-100"
                    : "bg-transparent border-transparent text-gray-300 hover:bg-gray-800/60 hover:border-gray-700/50"
                }`}
                onClick={() => !isRenaming && onSelect(s.id)}
              >
                <div className="flex items-start justify-between gap-1">
                  <div className="flex-1 min-w-0">
                    {isRenaming ? (
                      <input
                        type="text"
                        autoFocus
                        value={renameDraft}
                        onChange={(e) => setRenameDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitRename(s.id);
                          else if (e.key === "Escape") setRenamingId(null);
                        }}
                        onBlur={() => commitRename(s.id)}
                        onClick={(e) => e.stopPropagation()}
                        className="w-full bg-black/40 border border-indigo-500/40 rounded px-1.5 py-0.5 text-xs text-gray-100 focus:outline-none"
                      />
                    ) : (
                      <div className="font-medium truncate" title={s.title}>
                        {s.title}
                      </div>
                    )}
                    <div className="mt-0.5 flex items-center gap-1 text-[10px] text-gray-500">
                      <Folder size={9} className="shrink-0" />
                      <span
                        className="truncate"
                        title={`${s.explorer_path} · ${s.file_count} file${s.file_count === 1 ? "" : "s"}`}
                      >
                        {s.explorer_path.split(/[/\\]/).pop() || s.explorer_path}
                      </span>
                      <span className="text-gray-700">·</span>
                      <span className="whitespace-nowrap">
                        {relativeTime(s.last_active_at)}
                      </span>
                    </div>
                  </div>
                  {!isRenaming && (
                    <div className="opacity-0 group-hover:opacity-100 flex items-center gap-0.5 transition shrink-0">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setRenamingId(s.id);
                          setRenameDraft(s.title);
                        }}
                        className="p-1 rounded text-gray-500 hover:text-gray-200 hover:bg-gray-700/60"
                        title="Rename"
                      >
                        <Pencil size={10} />
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          if (confirm(`Delete session "${s.title}"?`)) {
                            onDelete(s.id);
                          }
                        }}
                        className="p-1 rounded text-gray-500 hover:text-red-400 hover:bg-red-900/30"
                        title="Delete"
                      >
                        <X size={10} />
                      </button>
                    </div>
                  )}
                </div>
              </div>
            );
          })
        )}
      </nav>
    </aside>
  );
}
