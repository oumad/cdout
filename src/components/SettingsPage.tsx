import { useState } from "react";
import {
  X,
  KeyRound,
  Sparkles,
  CheckCircle,
  XCircle,
  ExternalLink,
  Stethoscope,
  ShieldCheck,
} from "lucide-react";
import type {
  Skill,
  QuarantinedSkill,
  PlatformInfo,
  ApprovalMode,
} from "../types";
import { FileAccessDiagnostics } from "./FileAccessDiagnostics";

type SettingsCategory = "providers" | "approval" | "skills" | "diagnostics";

interface SettingsPageProps {
  // Local
  ollamaUrl: string;
  onOllamaUrlChange: (v: string) => void;

  // Cloud
  openrouterKey: string;
  anthropicKey: string;
  onOpenrouterKeyChange: (v: string) => void;
  onAnthropicKeyChange: (v: string) => void;
  /** Masked "…3fa1" preview if a key is already stored server-side. */
  openrouterPreview: string | null;
  anthropicPreview: string | null;
  showFreeOpenrouterModels: boolean;
  onToggleShowFreeOpenrouterModels: () => void;

  // Skills
  skills: Skill[];
  quarantinedSkills: QuarantinedSkill[];

  // Approval policy
  approvalMode: ApprovalMode;
  onApprovalModeChange: (mode: ApprovalMode) => void;

  // Diagnostics
  platform: PlatformInfo;
  onError?: (msg: string) => void;

  // Actions
  onSave: () => void;
  onClose: () => void;
}

const CATEGORIES: {
  id: SettingsCategory;
  label: string;
  icon: typeof KeyRound;
}[] = [
  { id: "providers", label: "Providers", icon: KeyRound },
  { id: "approval", label: "Approval", icon: ShieldCheck },
  { id: "skills", label: "Skills", icon: Sparkles },
  { id: "diagnostics", label: "Diagnostics", icon: Stethoscope },
];

export function SettingsPage(props: SettingsPageProps) {
  const [active, setActive] = useState<SettingsCategory>("providers");

  return (
    <div className="absolute top-8 left-0 right-0 bottom-0 z-40 flex bg-gray-950">
      {/* Sidebar */}
      <aside className="w-60 flex-none bg-gray-900/60 border-r border-gray-800 flex flex-col">
        <div className="px-5 py-4 border-b border-gray-800 flex items-center justify-between">
          <span className="text-sm font-semibold text-gray-200">Settings</span>
          <button
            onClick={props.onClose}
            className="p-1 rounded text-gray-500 hover:text-gray-200 hover:bg-gray-800 transition"
            title="Close (Esc)"
          >
            <X size={16} />
          </button>
        </div>
        <nav className="flex-1 overflow-y-auto py-3">
          {CATEGORIES.map((cat) => {
            const Icon = cat.icon;
            const isActive = active === cat.id;
            return (
              <button
                key={cat.id}
                onClick={() => setActive(cat.id)}
                className={`w-full flex items-center gap-3 px-5 py-2.5 text-sm transition ${
                  isActive
                    ? "bg-indigo-600/20 text-indigo-300 border-l-2 border-indigo-500"
                    : "text-gray-400 hover:text-gray-200 hover:bg-gray-800/50 border-l-2 border-transparent"
                }`}
              >
                <Icon size={16} />
                <span>{cat.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="p-4 border-t border-gray-800 flex gap-2">
          <button
            onClick={props.onSave}
            className="flex-1 py-2 bg-indigo-600 hover:bg-indigo-500 text-white rounded-md text-sm font-medium transition"
          >
            Save
          </button>
          <button
            onClick={props.onClose}
            className="px-4 py-2 bg-gray-800 hover:bg-gray-700 text-gray-300 border border-gray-700 rounded-md text-sm font-medium transition"
          >
            Close
          </button>
        </div>
      </aside>

      {/* Content */}
      <section className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-10 py-8">
          {active === "providers" && <ProvidersCategory {...props} />}
          {active === "approval" && (
            <ApprovalCategory
              mode={props.approvalMode}
              onChange={props.onApprovalModeChange}
              platform={props.platform}
            />
          )}
          {active === "skills" && (
            <SkillsSection
              skills={props.skills}
              quarantined={props.quarantinedSkills}
            />
          )}
          {active === "diagnostics" && (
            <>
              <SectionHeader
                title="Diagnostics"
                description={`Why the agent can or cannot see your ${props.platform.file_manager} selection.`}
              />
              <FileAccessDiagnostics
                platform={props.platform}
                onError={props.onError}
              />
              <div className="mt-8 text-xs text-gray-500 leading-relaxed space-y-1.5">
                <div>
                  Shell: <code className="text-gray-300">{props.platform.shell_name}</code>
                  {" · "}tool:{" "}
                  <code className="text-gray-300">{props.platform.shell_tool_name}</code>
                </div>
                <div>
                  Platform:{" "}
                  <code className="text-gray-300">{props.platform.os_name}</code>
                </div>
              </div>
            </>
          )}
        </div>
      </section>
    </div>
  );
}

function ProvidersCategory({
  ollamaUrl,
  onOllamaUrlChange,
  openrouterKey,
  anthropicKey,
  onOpenrouterKeyChange,
  onAnthropicKeyChange,
  openrouterPreview,
  anthropicPreview,
  showFreeOpenrouterModels,
  onToggleShowFreeOpenrouterModels,
}: SettingsPageProps) {
  return (
    <>
      <SectionHeader
        title="Providers"
        description="Configure where cdout sends model requests."
      />

      <SubSection
        title="Local"
        description="Self-hosted models on your own machine. Stays airtight — prompts never leave the box."
      >
        <Field
          label="Ollama Server URL"
          hint="Defaults to http://localhost:11434. Ollama models are prefixed `ollama:` in the picker."
        >
          <input
            type="text"
            value={ollamaUrl}
            onChange={(e) => onOllamaUrlChange(e.target.value)}
            className="w-full bg-black/40 border border-gray-700 rounded-md px-3.5 py-2.5 text-sm text-gray-200 focus:outline-none focus:border-indigo-500"
            placeholder="http://localhost:11434"
          />
        </Field>
      </SubSection>

      <SubSection
        title="Cloud"
        description="API keys for hosted providers. Stored locally in your config. Cloud prompts route through openrouter.ai by default; the Anthropic direct path is opt-in for prompt caching."
      >
        <Field
          label="OpenRouter API Key"
          hint={
            openrouterPreview
              ? `Current key on file: ${openrouterPreview}. Type a new value to replace.`
              : "Primary cloud gateway. Single key, ~400 models including Claude Opus/Sonnet 4.7+, GPT-5, Gemini 3."
          }
        >
          <input
            type="password"
            value={openrouterKey}
            onChange={(e) => onOpenrouterKeyChange(e.target.value)}
            className="w-full bg-black/40 border border-gray-700 rounded-md px-3.5 py-2.5 text-sm text-gray-200 focus:outline-none focus:border-indigo-500"
            placeholder={openrouterPreview ? `Saved: ${openrouterPreview}` : "sk-or-v1-..."}
          />
          <a
            href="https://openrouter.ai/keys"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 mt-2"
          >
            Get a key <ExternalLink size={10} />
          </a>
        </Field>

        <Field
          label="Anthropic API Key (optional)"
          hint={
            anthropicPreview
              ? `Current key on file: ${anthropicPreview}. Type a new value to replace.`
              : "Enables the direct Anthropic path with prompt caching — OpenRouter's OpenAI-compat wire drops cache_control. Surfaces `anthropic:claude-opus-4-7` and friends in the picker."
          }
        >
          <input
            type="password"
            value={anthropicKey}
            onChange={(e) => onAnthropicKeyChange(e.target.value)}
            className="w-full bg-black/40 border border-gray-700 rounded-md px-3.5 py-2.5 text-sm text-gray-200 focus:outline-none focus:border-indigo-500"
            placeholder={anthropicPreview ? `Saved: ${anthropicPreview}` : "sk-ant-api-..."}
          />
          <a
            href="https://console.anthropic.com/settings/keys"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 mt-2"
          >
            Get a key <ExternalLink size={10} />
          </a>
        </Field>

        <div className="mt-5 flex items-start justify-between gap-4 bg-gray-900/40 border border-gray-800 rounded-md p-4">
          <div className="flex flex-col min-w-0">
            <span className="text-sm font-medium text-gray-200">
              Show OpenRouter free-tier models
            </span>
            <span className="text-xs text-gray-500 mt-1 leading-relaxed">
              Off by default. Free models have documented FP4/Int4 quantization
              swaps that break non-Latin output, and ~20 req/min caps.
            </span>
          </div>
          <button
            onClick={onToggleShowFreeOpenrouterModels}
            className={`shrink-0 text-xs px-3 py-1.5 rounded-md border transition ${
              showFreeOpenrouterModels
                ? "bg-indigo-600/30 border-indigo-500/60 text-indigo-200"
                : "bg-gray-800 border-gray-600 text-gray-400 hover:bg-gray-700"
            }`}
          >
            {showFreeOpenrouterModels ? "Showing" : "Hidden"}
          </button>
        </div>
      </SubSection>
    </>
  );
}

function SectionHeader({
  title,
  description,
}: {
  title: string;
  description?: string;
}) {
  return (
    <div className="mb-8">
      <h2 className="text-xl font-semibold text-gray-100">{title}</h2>
      {description && (
        <p className="text-sm text-gray-500 mt-1.5">{description}</p>
      )}
    </div>
  );
}

function SubSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-10 pb-10 border-b border-gray-800/60 last:border-b-0 last:pb-0">
      <div className="mb-4">
        <h3 className="text-base font-semibold text-gray-200">{title}</h3>
        {description && (
          <p className="text-sm text-gray-500 mt-1 leading-relaxed">
            {description}
          </p>
        )}
      </div>
      {children}
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-5 last:mb-0">
      <label className="block text-sm font-medium text-gray-300 mb-2">
        {label}
      </label>
      {children}
      {hint && <p className="text-xs text-gray-500 mt-1.5">{hint}</p>}
    </div>
  );
}

const APPROVAL_OPTIONS: {
  id: ApprovalMode;
  label: string;
  detail: string;
  tone: "safe" | "default" | "risk";
}[] = [
  {
    id: "ask",
    label: "Ask every time",
    detail:
      "Nothing runs until you approve it. The most cautious setting, and the most clicks.",
    tone: "safe",
  },
  {
    id: "read_only",
    label: "Auto-run read-only commands",
    detail:
      "Probes that cannot change anything — reading a file's metadata, listing a folder, testing whether a path exists — run straight away. Anything that writes, moves or deletes still waits for you. A command the classifier does not recognise counts as writing, so the worst case is an extra prompt.",
    tone: "default",
  },
  {
    id: "auto",
    label: "Auto-run everything",
    detail:
      "The agent executes whatever it proposes, including commands that overwrite or delete files. Loop detection, the consecutive-failure pause and the step cap still apply, and recognisably destructive commands (rm -rf /, sudo, disk formatting) always stop for approval.",
    tone: "risk",
  },
];

function ApprovalCategory({
  mode,
  onChange,
  platform,
}: {
  mode: ApprovalMode;
  onChange: (mode: ApprovalMode) => void;
  platform: PlatformInfo;
}) {
  const modKey = platform.os === "macos" ? "\u2318" : "Ctrl";
  return (
    <>
      <SectionHeader
        title="Approval"
        description="How much cdout may run without asking. Every command is still shown and editable before it runs when approval is required."
      />
      <div className="space-y-2.5">
        {APPROVAL_OPTIONS.map((opt) => {
          const active = mode === opt.id;
          return (
            <button
              key={opt.id}
              onClick={() => onChange(opt.id)}
              className={`w-full text-left px-4 py-3 rounded-md border transition ${
                active
                  ? opt.tone === "risk"
                    ? "bg-red-900/20 border-red-500/40"
                    : "bg-indigo-600/15 border-indigo-500/40"
                  : "bg-gray-800/40 border-gray-700/50 hover:border-gray-600"
              }`}
            >
              <div className="flex items-center gap-2">
                <span
                  className={`w-3.5 h-3.5 rounded-full border flex-none ${
                    active
                      ? opt.tone === "risk"
                        ? "bg-red-500 border-red-400"
                        : "bg-indigo-500 border-indigo-400"
                      : "border-gray-600"
                  }`}
                />
                <span
                  className={`text-sm font-medium ${
                    active ? "text-gray-100" : "text-gray-300"
                  }`}
                >
                  {opt.label}
                </span>
                {opt.id === "read_only" && (
                  <span className="text-[10px] uppercase tracking-wider text-gray-500">
                    default
                  </span>
                )}
              </div>
              <p className="text-xs text-gray-400 leading-relaxed mt-1.5 pl-[22px]">
                {opt.detail}
              </p>
            </button>
          );
        })}
      </div>
      <p className="text-xs text-gray-500 leading-relaxed mt-6">
        Per-task override: press{" "}
        <code className="text-gray-300">{modKey}+Enter</code> in the spotlight
        to run one task unattended without changing this setting — that way you
        do not have to wait for the first proposal just to click{" "}
        <strong>All</strong>.
      </p>
    </>
  );
}

function SkillsSection({
  skills,
  quarantined,
}: {
  skills: Skill[];
  quarantined: QuarantinedSkill[];
}) {
  return (
    <>
      <SectionHeader
        title="Skills"
        description="Capabilities injected into the agent's system prompt. Availability depends on whether required binaries are on PATH."
      />
      {skills.length === 0 ? (
        <p className="text-sm text-gray-500">No skills loaded.</p>
      ) : (
        <div className="space-y-2.5">
          {skills.map((skill) => (
            <div
              key={skill.metadata.name}
              className={`flex items-start justify-between gap-4 px-4 py-3 rounded-md border ${
                skill.available
                  ? "bg-emerald-900/20 border-emerald-500/30"
                  : "bg-gray-800/40 border-gray-700/50"
              }`}
            >
              <div className="flex flex-col min-w-0">
                <span
                  className={`text-sm font-medium ${
                    skill.available ? "text-emerald-300" : "text-gray-400"
                  }`}
                >
                  {skill.metadata.name}
                </span>
                <span className="text-xs text-gray-500 mt-1">
                  {skill.metadata.description}
                </span>
              </div>
              {skill.available ? (
                <CheckCircle size={14} className="text-emerald-500 shrink-0 mt-0.5" />
              ) : (
                <div className="flex items-center gap-1.5 shrink-0 mt-0.5">
                  <XCircle size={14} className="text-gray-600" />
                  <span className="text-xs text-gray-600">
                    Missing: {skill.missing_bins.join(", ")}
                  </span>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {quarantined.length > 0 && (
        <div className="mt-8">
          <h3 className="text-base font-semibold text-red-300 mb-2">
            Quarantined skills ({quarantined.length})
          </h3>
          <p className="text-xs text-gray-500 mb-4 leading-relaxed">
            These user-supplied skills were flagged by the safety scanner and
            are <strong>not</strong> being loaded into the agent. Each hit lists
            the rule that matched. Review the source file and remove the
            offending pattern if the skill is legitimate.
          </p>
          <div className="space-y-2.5">
            {quarantined.map((q) => (
              <div
                key={q.source_path}
                className="px-4 py-3 rounded-md border bg-red-900/20 border-red-500/30"
              >
                <div className="flex items-start justify-between gap-4 mb-2">
                  <div className="flex flex-col min-w-0">
                    <span className="text-sm font-medium text-red-300">
                      {q.metadata.name}
                    </span>
                    <span
                      className="text-xs text-gray-500 mt-1 break-all"
                      title={q.source_path}
                    >
                      {q.source_path}
                    </span>
                  </div>
                  <XCircle size={14} className="text-red-400 shrink-0 mt-0.5" />
                </div>
                <ul className="text-xs text-red-200 space-y-1 mt-2 pl-2 border-l border-red-500/30">
                  {q.safety_hits.map((h, i) => (
                    <li key={i} className="flex items-start gap-2">
                      <span className="font-mono text-red-400">{h.rule}</span>
                      <span className="text-gray-400">— matched</span>
                      <code className="bg-black/40 px-1 rounded text-gray-300">
                        {h.snippet}
                      </code>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
