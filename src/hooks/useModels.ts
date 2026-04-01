import { useState, useEffect, useCallback } from "react";
import { MODEL_POLL_MS } from "../constants";
import * as api from "../utils/tauri";

export function useModels(ollamaUrl?: string) {
  const [modelName, setModelName] = useState("");
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [ollamaConnected, setOllamaConnected] = useState<boolean | null>(null);

  // Load persisted model on mount
  useEffect(() => {
    api.getSelectedModel().then((saved) => {
      if (saved) setModelName(saved);
    }).catch(() => {});
  }, []);

  const fetchModels = useCallback(async () => {
    try {
      const models = await api.getOllamaModels();
      setAvailableModels(models);
      setOllamaConnected(true);
      // Only default to first model if no saved model was loaded and current is empty
      setModelName((prev) => (models.length > 0 && !prev ? models[0] : prev));
    } catch {
      setOllamaConnected(false);
      setAvailableModels([]);
    }
  }, []);

  // Persist model selection on change
  const setModelNameAndSave = useCallback((model: string) => {
    setModelName(model);
    if (model) {
      api.setSelectedModel(model).catch(() => {});
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
    setModelName: setModelNameAndSave,
    availableModels,
    ollamaConnected,
    setOllamaConnected,
    fetchModels,
  };
}
