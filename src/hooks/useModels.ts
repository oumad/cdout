import { useState, useEffect, useCallback } from "react";
import { MODEL_POLL_MS } from "../constants";
import * as api from "../utils/tauri";

export function useModels(ollamaUrl?: string) {
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null);

  const fetchModels = useCallback(async () => {
    try {
      const models = await api.getOllamaModels();
      setAvailableModels(models);
      setOllamaConnected(true);
      setModelName((prev) => (models.length > 0 && !prev ? models[0] : prev));
    } catch {
      setOllamaConnected(false);
      setAvailableModels([]);
    }
  }, []);

  // Initial fetch (re-runs when ollamaUrl changes, if provided)
  useEffect(() => {
    fetchModels();
  }, [fetchModels, ollamaUrl]);

  // Periodic polling when disconnected or no models
  useEffect(() => {
    if (ollamaConnected === false || (ollamaConnected === true && availableModels.length === 0)) {
      const interval = setInterval(fetchModels, MODEL_POLL_MS);
      return () => clearInterval(interval);
    }
  }, [ollamaConnected, availableModels.length, fetchModels]);

  return {
    modelName,
    setModelName,
    availableModels,
    ollamaConnected,
    setOllamaConnected,
    fetchModels,
  };
}
