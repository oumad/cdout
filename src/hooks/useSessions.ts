import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../utils/tauri";
import type { Message, Session, SessionMeta } from "../types";

interface UseSessionsReturn {
  sessions: SessionMeta[];
  currentSessionId: string | null;
  setCurrentSessionId: (id: string | null) => void;
  refresh: () => Promise<void>;
  newSession: (
    userPrompt: string,
    contextPath: string,
    selectedFiles: string[],
    model: string
  ) => Promise<Session>;
  loadSession: (id: string) => Promise<Session>;
  deleteSession: (id: string) => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  /**
   * Save the current chat history into the session file. Debounced internally
   * so the on-disk write happens at most every SAVE_DEBOUNCE_MS regardless of
   * how often messages stream in.
   */
  scheduleSave: (messages: Message[]) => void;
  /** Flush any pending debounced save immediately (e.g. before switching sessions). */
  flushSave: () => Promise<void>;
}

const SAVE_DEBOUNCE_MS = 500;

export function useSessions(): UseSessionsReturn {
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listSessions();
      setSessions(list);
    } catch (e) {
      console.error("listSessions failed", e);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const newSession = useCallback(
    async (
      userPrompt: string,
      contextPath: string,
      selectedFiles: string[],
      model: string
    ) => {
      const s = await api.createSession(userPrompt, contextPath, selectedFiles, model);
      setCurrentSessionId(s.id);
      await refresh();
      return s;
    },
    [refresh]
  );

  const loadSession = useCallback(
    async (id: string) => {
      const s = await api.loadSession(id);
      setCurrentSessionId(s.id);
      return s;
    },
    []
  );

  const deleteSession = useCallback(
    async (id: string) => {
      await api.deleteSession(id);
      setCurrentSessionId((cur) => (cur === id ? null : cur));
      await refresh();
    },
    [refresh]
  );

  const renameSession = useCallback(
    async (id: string, title: string) => {
      await api.renameSession(id, title);
      await refresh();
    },
    [refresh]
  );

  // Debounced save state.
  const pendingSaveRef = useRef<{
    id: string;
    messages: Message[];
  } | null>(null);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flushSave = useCallback(async () => {
    if (saveTimerRef.current) {
      clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    const pending = pendingSaveRef.current;
    if (!pending) return;
    pendingSaveRef.current = null;
    try {
      await api.saveSessionMessages(pending.id, pending.messages);
      // Re-list so the sidebar reflects the bumped last_active_at + message_count.
      await refresh();
    } catch (e) {
      console.error("saveSessionMessages failed", e);
    }
  }, [refresh]);

  const scheduleSave = useCallback(
    (messages: Message[]) => {
      if (!currentSessionId) return;
      pendingSaveRef.current = { id: currentSessionId, messages };
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
      saveTimerRef.current = setTimeout(() => {
        // Fire-and-forget — flushSave handles its own errors.
        flushSave();
      }, SAVE_DEBOUNCE_MS);
    },
    [currentSessionId, flushSave]
  );

  // Flush any pending save on unmount so we don't lose the last 500ms.
  useEffect(() => {
    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
        const pending = pendingSaveRef.current;
        if (pending) {
          // Best-effort fire-and-forget — we're in cleanup.
          api.saveSessionMessages(pending.id, pending.messages).catch(() => {});
        }
      }
    };
  }, []);

  return {
    sessions,
    currentSessionId,
    setCurrentSessionId,
    refresh,
    newSession,
    loadSession,
    deleteSession,
    renameSession,
    scheduleSave,
    flushSave,
  };
}
