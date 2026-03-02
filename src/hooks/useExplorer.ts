import { useState, useCallback } from "react";
import type { ExplorerState } from "../types";
import * as api from "../utils/tauri";

export function useExplorer() {
  const [explorerState, setExplorerState] = useState<ExplorerState | null>(null);

  const fetchExplorer = useCallback(async () => {
    try {
      const state = await api.getExplorerStatus();
      setExplorerState(state);
      return state;
    } catch {
      setExplorerState(null);
      return null;
    }
  }, []);

  return { explorerState, setExplorerState, fetchExplorer };
}
