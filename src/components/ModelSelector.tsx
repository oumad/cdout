interface ModelSelectorProps {
  modelName: string;
  onChange: (model: string) => void;
  models: string[];
  connected: boolean | null;
  disabled?: boolean;
  className?: string;
  /**
   * Max characters shown for any single option/value. Long Ollama slugs
   * (e.g. `ollama:hf.co/some-org/very-long-model-name:latest`) otherwise
   * widen the native dropdown and spill out of the spotlight window.
   * Default 36 — short enough to fit a 600px spotlight, long enough that
   * the meaningful part of typical model names is preserved.
   */
  maxDisplayChars?: number;
}

/**
 * Group models by provider so users can see WHY two `claude-opus-4.7`-shaped
 * entries exist (direct Anthropic with prompt caching vs OpenRouter slug).
 */
function groupModels(models: string[]) {
  const local: string[] = [];
  const anthropicDirect: string[] = [];
  const openrouter: string[] = [];
  for (const m of models) {
    if (m.startsWith("ollama:")) local.push(m);
    else if (m.startsWith("anthropic:")) anthropicDirect.push(m);
    else openrouter.push(m);
  }
  return { local, anthropicDirect, openrouter };
}

/**
 * Compact a model id so it fits in narrow UIs. Keeps the provider prefix and
 * the model's trailing identity (tag / version) intact, ellipsising the
 * middle. `ollama:hf.co/org/long-model-name:latest` → `ollama:hf.co/…me:latest`.
 */
function truncateDisplay(slug: string, max: number): string {
  if (slug.length <= max) return slug;
  // Reserve 1 char for the ellipsis.
  const keep = max - 1;
  const head = Math.ceil(keep * 0.55);
  const tail = keep - head;
  return slug.slice(0, head) + "…" + slug.slice(slug.length - tail);
}

export function ModelSelector({
  modelName,
  onChange,
  models,
  connected,
  disabled = false,
  className = "",
  maxDisplayChars = 36,
}: ModelSelectorProps) {
  // Render the model list whenever we have any models, regardless of whether
  // Ollama is reachable — cloud-only setups must still be usable. Reserve the
  // "No models" label for the genuinely-empty case.
  const hasModels = models.length > 0;
  const groups = groupModels(models);

  // The value sent to the router stays the full prefixed slug — only the
  // VISIBLE text drops the prefix when its optgroup already conveys it.
  // OpenRouter slugs (`anthropic/claude-opus-4.7`, `openai/gpt-5`) keep their
  // `provider/model` shape because that IS the OpenRouter convention.
  const renderOption = (m: string, displayText: string) => (
    <option key={m} value={m} title={m}>
      {truncateDisplay(displayText, maxDisplayChars)}
    </option>
  );

  return (
    <select
      className={`bg-transparent focus:outline-none cursor-pointer max-w-full ${className}`}
      style={{ maxWidth: "100%" }}
      value={modelName}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled || !hasModels}
      title={modelName}
    >
      {!hasModels ? (
        connected === null ? (
          <option value="">Checking...</option>
        ) : (
          <option value="">
            No models — configure a provider in Settings
          </option>
        )
      ) : (
        <>
          {groups.local.length > 0 && (
            <optgroup label="Local (Ollama)">
              {groups.local.map((m) =>
                renderOption(m, m.replace(/^ollama:/, ""))
              )}
            </optgroup>
          )}
          {groups.anthropicDirect.length > 0 && (
            <optgroup label="Anthropic (direct · cached)">
              {groups.anthropicDirect.map((m) =>
                renderOption(m, m.replace(/^anthropic:/, ""))
              )}
            </optgroup>
          )}
          {groups.openrouter.length > 0 && (
            <optgroup label="OpenRouter">
              {groups.openrouter.map((m) => renderOption(m, m))}
            </optgroup>
          )}
        </>
      )}
    </select>
  );
}
