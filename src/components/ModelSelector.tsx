interface ModelSelectorProps {
  modelName: string;
  onChange: (model: string) => void;
  models: string[];
  connected: boolean | null;
  disabled?: boolean;
  className?: string;
}

export function ModelSelector({
  modelName,
  onChange,
  models,
  connected,
  disabled = false,
  className = "",
}: ModelSelectorProps) {
  return (
    <select
      className={`bg-transparent focus:outline-none cursor-pointer ${className}`}
      value={modelName}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled || models.length === 0}
    >
      {connected === null ? (
        <option value="">Checking...</option>
      ) : connected === false ? (
        <option value="">No Ollama</option>
      ) : models.length === 0 ? (
        <option value="">No models</option>
      ) : (
        models.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))
      )}
    </select>
  );
}
