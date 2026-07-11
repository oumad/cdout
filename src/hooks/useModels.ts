import { useState, useEffect, useCallback, useRef } from "react";
import { MODEL_POLL_MS } from "../constants";
import * as api from "../utils/tauri";

/**
 * When the saved selection isn't in the new available list (post-migration
 * legacy slugs like `claude:claude-sonnet-4-20250514`, or a removed API key
 * dropping its slugs), pick the best replacement instead of silently
 * downgrading to a random Ollama model. Preference order:
 *   1. anthropic/* (OpenRouter slug)
 *   2. anthropic:* (direct Anthropic)
 *   3. openai/*  (OpenRouter slug)
 *   4. google/*  (OpenRouter slug)
 *   5. ollama:*  (local fallback — only if nothing else matches)
 *   6. the first model in the list
 *
 * Honors the cloud-first intent: if the user had a Claude or OpenAI slug
 * saved, they probably don't want an Ollama 7B replacement when an OR key is
 * configured.
 */
function pickReplacement(models: string[]): string | undefined {
  if (models.length === 0) return undefined;
  const findPrefix = (p: string) => models.find((m) => m.startsWith(p));
  return (
    findPrefix("anthropic/") ??
    findPrefix("anthropic:") ??
    findPrefix("openai/") ??
    findPrefix("google/") ??
    findPrefix("ollama:") ??
    models[0]
  );
}

export interface UseModelsOptions {
  /** Toast hook injected from the host component for stale-slug notices. */
  onSwap?: (prev: string, next: string) => void;
}

/**
 * useModels — polls the backend for the current model list and reconciles
 * the persisted selection.
 *
 * ⚠ Historical bug: `opts` used to be in `fetchModels`'s useCallback deps.
 * Callers pass a fresh object literal `{ onSwap: (...) => ... }` on every
 * render, so `opts` identity changed each render → `fetchModels` identity
 * changed each render → `useEffect(fetchModels, ollamaUrl)` fired on every
 * render → during streaming (setState per token) shuttle-io opened ~80
 * TCP connections per second to Ollama, drained the Windows ephemeral port
 * pool, and broke every other process on the machine.
 *
 * The fix has three parts:
 *   1) `onSwapRef` — latest callback but stable ref identity. Assigned
 *      inside a `useEffect`, NOT during render, so it only tracks committed
 *      renders (concurrent-mode safe).
 *   2) `fetchModels` has EMPTY deps, so the callback identity is stable.
 *   3) An in-flight guard collapses overlapping calls (e.g. the initial
 *      fetch and a manual refresh firing on the same render, and React
 *      StrictMode's dev-only double-mount of the initial useEffect).
 *
 * The setModelName updater is PURE — side effects (API IPC + onSwap toast)
 * fire after `setModelName` returns, not inside the updater callback. This
 * matters because React 18 StrictMode double-invokes updater functions in
 * dev to surface impurity; keeping the updater pure prevents duplicate IPC
 * writes and duplicate toast fires.
 */
export function useModels(ollamaUrl?: string, opts: UseModelsOptions = {}) {
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null);

  // Keep the LATEST onSwap without churning fetchModels' identity every
  // render. Assigned inside useEffect (after commit), not during render, so
  // a concurrent render that gets thrown away doesn't leave the ref pointing
  // at a discarded callback.
  const onSwapRef = useRef(opts.onSwap);
  useEffect(() => {
    onSwapRef.current = opts.onSwap;
  });

  // Mirror of committed `modelName` so fetchModels can read the latest value
  // without going through a state updater callback. This is what lets the
  // whole reconciliation live OUTSIDE setState — side effects (IPC persist
  // + onSwap toast) fire exactly once even under React.StrictMode dev
  // double-invocation, because they're no longer coupled to updater purity.
  const modelNameRef = useRef(modelName);
  useEffect(() => {
    modelNameRef.current = modelName;
  });

  // Reentry guard — overlapping fetches collapse to one in-flight request.
  // Also collapses React StrictMode's dev double-mount to a single network
  // call, which the regression test verifies.
  const inflight = useRef(false);

  // Has the persisted-model read completed? Until it has, we must NOT persist
  // an auto-picked default — doing so would clobber the user's saved model on
  // disk in the mount race where fetchModels resolves before getSelectedModel.
  const hydrated = useRef(false);

  // Load persisted model on mount
  useEffect(() => {
    api
      .getSelectedModel()
      .then((saved) => {
        hydrated.current = true;
        if (saved) {
          // Saved model wins over any transient auto-pick fetchModels showed.
          // Sync the mirror ref immediately so a concurrent reconcile reads
          // the right value. It's already on disk, so no persist needed.
          modelNameRef.current = saved;
          setModelName(saved);
        } else {
          // Disk was genuinely empty — persist whatever we're currently
          // showing (an auto-pick made before hydration).
          const cur = modelNameRef.current;
          if (cur) api.setSelectedModel(cur).catch(() => {});
        }
      })
      .catch(() => {
        hydrated.current = true;
      });
  }, []);

  const fetchModels = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      const models = await api.getOllamaModels();
      setAvailableModels(models);
      setOllamaConnected(true);
      if (models.length === 0) return;

      // Read the CURRENT model via the mirror ref, decide, then dispatch
      // side effects — no state updater callback involved, so React.StrictMode
      // double-invocation cannot fire IPC / toast twice.
      const currentModel = modelNameRef.current;
      if (!currentModel) {
        // Show a model immediately so the UI isn't blank, but only PERSIST it
        // once hydration has confirmed the disk had no saved selection. Before
        // hydration, the persist is deferred to the hydration effect.
        const next = pickReplacement(models) ?? models[0];
        setModelName(next);
        if (hydrated.current) {
          api.setSelectedModel(next).catch(() => {});
        }
      } else if (!models.includes(currentModel)) {
        // currentModel is non-empty, so it came from disk or the user — a
        // genuine stale-slug swap. Safe to persist immediately.
        const next = pickReplacement(models) ?? models[0];
        setModelName(next);
        onSwapRef.current?.(currentModel, next);
        api.setSelectedModel(next).catch(() => {});
      }
    } catch {
      setOllamaConnected(false);
      setAvailableModels([]);
    } finally {
      inflight.current = false;
    }
    // Empty deps → stable identity. onSwap + modelName are read through refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Persist model selection on change
  const setModelNameAndSave = useCallback((model: string) => {
    setModelName(model);
    if (model) {
      api.setSelectedModel(model).catch(() => {});
    }
  }, []);

  // Initial fetch + re-fetch only when ollamaUrl actually changes.
  useEffect(() => {
    fetchModels();
    // fetchModels is stable per the useCallback above; excluding it keeps
    // this effect from firing every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ollamaUrl]);

  // Periodic polling ONLY when disconnected or when we have zero models.
  // Once we have a healthy model list, this interval stops.
  useEffect(() => {
    const shouldPoll =
      ollamaConnected === false ||
      (ollamaConnected === true && availableModels.length === 0);
    if (!shouldPoll) return;
    const interval = setInterval(fetchModels, MODEL_POLL_MS);
    return () => clearInterval(interval);
    // Same stable-fetchModels rationale as above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ollamaConnected, availableModels.length]);

  return {
    modelName,
    setModelName: setModelNameAndSave,
    availableModels,
    ollamaConnected,
    setOllamaConnected,
    fetchModels,
  };
}
