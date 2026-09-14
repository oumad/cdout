import { useState, useEffect, useCallback, useRef } from "react";
import { Keyboard, Loader2, RotateCcw } from "lucide-react";
import type { PlatformInfo } from "../types";

/**
 * Shows the current spotlight hotkey and records a new one.
 *
 * Records a real chord rather than offering a text field: the accepted syntax
 * is Tauri's (`Cmd+Alt+A`, `Ctrl+Shift+F5`), and asking a user to guess it —
 * then telling them it failed to parse — is a worse experience than pressing
 * the keys.
 */

const IS_MAC_GLYPH: Record<string, string> = {
  Cmd: "⌘",
  Alt: "⌥",
  Ctrl: "⌃",
  Shift: "⇧",
};

/** Turn a stored shortcut string into display-friendly key names. */
export function prettifyHotkey(hotkey: string, isMac: boolean): string[] {
  return hotkey.split("+").map((part) => {
    const token = part.trim();
    if (isMac && IS_MAC_GLYPH[token]) return IS_MAC_GLYPH[token];
    if (token === "CommandOrControl" || token === "CmdOrCtrl") {
      return isMac ? IS_MAC_GLYPH.Cmd : "Ctrl";
    }
    if (token === "Super") return isMac ? IS_MAC_GLYPH.Cmd : "Win";
    // "Digit1" → "1", "KeyA" → "A", "ArrowUp" → "↑"
    if (token.startsWith("Digit")) return token.slice(5);
    if (token.startsWith("Key")) return token.slice(3);
    const arrows: Record<string, string> = {
      ArrowUp: "↑",
      ArrowDown: "↓",
      ArrowLeft: "←",
      ArrowRight: "→",
    };
    return arrows[token] ?? token;
  });
}

/** Modifier order used when building and comparing chords. */
const MOD_ORDER = ["Ctrl", "Alt", "Shift", "Cmd", "Super"];

/**
 * Reduce a shortcut string to a comparable form.
 *
 * `Cmd+Alt+A` and `Alt+Cmd+A` are the same chord but different strings, and
 * the recorder always emits its own modifier order — so a plain string
 * compare would treat re-recording the current hotkey as a change, save a
 * reordered duplicate, and light up "Reset" for a hotkey that *is* the
 * default.
 */
export function canonicalizeHotkey(hotkey: string, isMac: boolean): string {
  const mods: string[] = [];
  const keys: string[] = [];
  for (const raw of hotkey.split("+")) {
    let t = raw.trim();
    if (!t) continue;
    if (t === "Control") t = "Ctrl";
    if (t === "Command" || t === "Meta") t = "Cmd";
    if (t === "Option") t = "Alt";
    if (t === "CommandOrControl" || t === "CmdOrCtrl") t = isMac ? "Cmd" : "Ctrl";
    if (t === "Super" && isMac) t = "Cmd";
    if (MOD_ORDER.includes(t)) {
      if (!mods.includes(t)) mods.push(t);
    } else {
      keys.push(/^Key[A-Z]$/.test(t) ? t.slice(3) : t.length === 1 ? t.toUpperCase() : t);
    }
  }
  mods.sort((a, b) => MOD_ORDER.indexOf(a) - MOD_ORDER.indexOf(b));
  return [...mods, ...keys].join("+");
}

/**
 * Build a Tauri shortcut string from a keydown event, or explain why the
 * chord is unusable. Bare keys are rejected: this binding is *global*, so a
 * modifier-less hotkey would swallow that key in every other application.
 */
export function chordFromEvent(
  e: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
  isMac: boolean
): { hotkey: string } | { error: string } {
  const code = e.code;

  // Modifier-only presses are the user mid-chord, not a finished one.
  if (
    /^(Control|Alt|Shift|Meta|OS)(Left|Right)$/.test(code) ||
    code === "CapsLock" ||
    code === "Fn"
  ) {
    return { error: "" };
  }

  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push(isMac ? "Cmd" : "Super");

  if (mods.length === 0) {
    return {
      error: "Needs at least one modifier — a bare key would be captured system-wide.",
    };
  }

  // Letters are emitted bare ("A") to match the shipped default's style;
  // everything else keeps its W3C code name, which the backend parses.
  let key = code;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);

  return { hotkey: [...mods, key].join("+") };
}

export function HotkeyRecorder({
  hotkey,
  platform,
  onSave,
  defaultHotkey,
}: {
  hotkey: string;
  platform: PlatformInfo;
  /** Rejects with the backend's message when the OS refuses the chord. */
  onSave: (hotkey: string) => Promise<void>;
  defaultHotkey: string;
}) {
  const [recording, setRecording] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isMac = platform.os === "macos";
  // Read inside the listener without re-subscribing on every keystroke.
  const savingRef = useRef(saving);
  savingRef.current = saving;

  const commit = useCallback(
    async (next: string) => {
      setRecording(false);
      // Canonical compare: re-pressing the current chord is not a change even
      // when the recorder spells its modifiers in a different order.
      if (canonicalizeHotkey(next, isMac) === canonicalizeHotkey(hotkey, isMac)) {
        return;
      }
      setSaving(true);
      setError(null);
      try {
        await onSave(next);
      } catch (e) {
        setError(String(e));
      } finally {
        setSaving(false);
      }
    },
    [hotkey, isMac, onSave]
  );

  useEffect(() => {
    if (!recording) return;
    function onKeyDown(e: KeyboardEvent) {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        setRecording(false);
        setError(null);
        return;
      }
      if (savingRef.current) return;
      const result = chordFromEvent(e, isMac);
      if ("error" in result) {
        // Empty message means "still holding modifiers" — not a complaint.
        if (result.error) setError(result.error);
        return;
      }
      setError(null);
      void commit(result.hotkey);
    }
    // Capture phase, so the chord never reaches the app's own shortcuts.
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [recording, isMac, commit]);

  const keys = prettifyHotkey(hotkey, isMac);

  return (
    <div>
      <div className="flex items-center gap-2.5">
        <button
          onClick={() => {
            setError(null);
            setRecording((r) => !r);
          }}
          disabled={saving}
          aria-label="Change the spotlight hotkey"
          className={`flex items-center gap-2 px-3 py-2 rounded-md border text-sm transition disabled:opacity-50 ${
            recording
              ? "bg-indigo-600/20 border-indigo-500/50 text-indigo-200 animate-pulse"
              : "bg-black/40 border-gray-700 text-gray-200 hover:border-gray-600"
          }`}
        >
          {saving ? (
            <Loader2 size={14} className="animate-spin" />
          ) : (
            <Keyboard size={14} className="text-gray-500" />
          )}
          {recording ? (
            <span className="text-xs">Press the keys… (Esc to cancel)</span>
          ) : (
            <span className="flex items-center gap-1">
              {keys.map((k, i) => (
                <span key={i} className="flex items-center gap-1">
                  {i > 0 && <span className="text-gray-600 text-xs">+</span>}
                  <kbd className="px-1.5 py-0.5 rounded bg-gray-800 border border-gray-700 text-xs font-mono text-gray-200">
                    {k}
                  </kbd>
                </span>
              ))}
            </span>
          )}
        </button>

        {canonicalizeHotkey(hotkey, isMac) !==
          canonicalizeHotkey(defaultHotkey, isMac) &&
          !recording && (
          <button
            onClick={() => commit(defaultHotkey)}
            disabled={saving}
            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 text-xs text-gray-400 hover:text-gray-200 transition disabled:opacity-50"
            title={`Reset to ${defaultHotkey}`}
          >
            <RotateCcw size={12} />
            Reset
          </button>
          )}
      </div>

      {error && (
        <p className="text-xs text-red-300 mt-2 leading-relaxed">{error}</p>
      )}
    </div>
  );
}
