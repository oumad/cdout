import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import {
  HotkeyRecorder,
  chordFromEvent,
  prettifyHotkey,
  canonicalizeHotkey,
} from "./HotkeyRecorder";
import type { PlatformInfo } from "../types";

const MAC: PlatformInfo = {
  os: "macos",
  os_name: "macOS",
  file_manager: "Finder",
  shell_name: "zsh",
  shell_tool_name: "run_shell",
  read_list_hint: "",
  default_hotkey: "Cmd+Alt+A",
};

const WINDOWS: PlatformInfo = { ...MAC, os: "windows", default_hotkey: "Ctrl+Alt+A" };

type Chord = Parameters<typeof chordFromEvent>[0];
const ev = (over: Partial<Chord>): Chord => ({
  code: "KeyA",
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...over,
});

describe("chordFromEvent", () => {
  it("builds a Tauri shortcut string in modifier order", () => {
    expect(
      chordFromEvent(ev({ code: "KeyA", metaKey: true, altKey: true }), true)
    ).toEqual({ hotkey: "Alt+Cmd+A" });
    expect(
      chordFromEvent(ev({ code: "KeyP", ctrlKey: true, shiftKey: true }), false)
    ).toEqual({ hotkey: "Ctrl+Shift+P" });
  });

  it("maps the Meta key per platform", () => {
    // Tauri spells the Windows key "Super"; on macOS it is "Cmd".
    expect(chordFromEvent(ev({ metaKey: true }), true)).toEqual({
      hotkey: "Cmd+A",
    });
    expect(chordFromEvent(ev({ metaKey: true }), false)).toEqual({
      hotkey: "Super+A",
    });
  });

  it("refuses a bare key, which would be swallowed system-wide", () => {
    const r = chordFromEvent(ev({ code: "KeyA" }), true);
    expect(r).toHaveProperty("error");
    expect("error" in r && r.error).toMatch(/modifier/i);
  });

  it("ignores a modifier held on its own as mid-chord, not an error", () => {
    for (const code of ["ControlLeft", "AltRight", "ShiftLeft", "MetaLeft"]) {
      const r = chordFromEvent(ev({ code, ctrlKey: true }), true);
      // Empty message = "keep waiting", so no complaint is shown.
      expect(r).toEqual({ error: "" });
    }
  });

  it("keeps W3C code names for non-letters so the backend can parse them", () => {
    expect(chordFromEvent(ev({ code: "F5", ctrlKey: true }), true)).toEqual({
      hotkey: "Ctrl+F5",
    });
    expect(chordFromEvent(ev({ code: "Digit1", altKey: true }), true)).toEqual({
      hotkey: "Alt+Digit1",
    });
    expect(chordFromEvent(ev({ code: "Space", ctrlKey: true }), true)).toEqual({
      hotkey: "Ctrl+Space",
    });
  });
});

describe("prettifyHotkey", () => {
  it("uses mac glyphs on macOS and words elsewhere", () => {
    expect(prettifyHotkey("Cmd+Alt+A", true)).toEqual(["⌘", "⌥", "A"]);
    expect(prettifyHotkey("Ctrl+Alt+A", false)).toEqual(["Ctrl", "Alt", "A"]);
  });

  it("expands code names a user should not have to read", () => {
    expect(prettifyHotkey("Ctrl+Digit1", false)).toEqual(["Ctrl", "1"]);
    expect(prettifyHotkey("Ctrl+ArrowUp", false)).toEqual(["Ctrl", "↑"]);
    expect(prettifyHotkey("Ctrl+KeyB", false)).toEqual(["Ctrl", "B"]);
  });

  it("resolves the cross-platform aliases", () => {
    expect(prettifyHotkey("CommandOrControl+A", true)).toEqual(["⌘", "A"]);
    expect(prettifyHotkey("CommandOrControl+A", false)).toEqual(["Ctrl", "A"]);
    expect(prettifyHotkey("Super+A", false)).toEqual(["Win", "A"]);
  });
});

describe("canonicalizeHotkey", () => {
  it("treats reordered modifiers as the same chord", () => {
    expect(canonicalizeHotkey("Cmd+Alt+A", true)).toBe(
      canonicalizeHotkey("Alt+Cmd+A", true)
    );
  });

  it("folds aliases and letter spellings together", () => {
    expect(canonicalizeHotkey("Control+Option+KeyA", true)).toBe("Ctrl+Alt+A");
    expect(canonicalizeHotkey("CmdOrCtrl+a", false)).toBe("Ctrl+A");
    expect(canonicalizeHotkey("Super+A", true)).toBe("Cmd+A");
  });

  it("keeps genuinely different chords distinct", () => {
    expect(canonicalizeHotkey("Cmd+Alt+A", true)).not.toBe(
      canonicalizeHotkey("Cmd+Alt+B", true)
    );
    expect(canonicalizeHotkey("Cmd+A", true)).not.toBe(
      canonicalizeHotkey("Cmd+Shift+A", true)
    );
  });
});

describe("HotkeyRecorder", () => {
  function setup(over: Partial<Parameters<typeof HotkeyRecorder>[0]> = {}) {
    const props = {
      hotkey: "Cmd+Alt+A",
      platform: MAC,
      onSave: vi.fn<(h: string) => Promise<void>>(async () => undefined),
      defaultHotkey: "Cmd+Alt+A",
      ...over,
    };
    render(<HotkeyRecorder {...props} />);
    return props;
  }

  it("shows the current hotkey without being asked", () => {
    // The actual complaint that prompted this: there was no way to even see it.
    setup();
    expect(screen.getByText("⌘")).toBeInTheDocument();
    expect(screen.getByText("⌥")).toBeInTheDocument();
    expect(screen.getByText("A")).toBeInTheDocument();
  });

  it("records a new chord and saves it", async () => {
    const props = setup();
    fireEvent.click(screen.getByLabelText(/Change the spotlight hotkey/i));
    expect(screen.getByText(/Press the keys/i)).toBeInTheDocument();
    fireEvent.keyDown(window, { code: "KeyJ", metaKey: true, shiftKey: true });
    await waitFor(() =>
      expect(props.onSave).toHaveBeenCalledWith("Shift+Cmd+J")
    );
  });

  it("cancels on Escape without saving", async () => {
    const props = setup();
    fireEvent.click(screen.getByLabelText(/Change the spotlight hotkey/i));
    fireEvent.keyDown(window, { code: "Escape" });
    expect(screen.queryByText(/Press the keys/i)).not.toBeInTheDocument();
    expect(props.onSave).not.toHaveBeenCalled();
  });

  it("surfaces the backend's refusal and keeps the old hotkey visible", async () => {
    // The OS rejects a chord another app owns; the user needs to be told.
    const props = setup({
      onSave: vi.fn(async () => {
        throw "Could not register 'Ctrl+Space': already in use.";
      }),
    });
    fireEvent.click(screen.getByLabelText(/Change the spotlight hotkey/i));
    fireEvent.keyDown(window, { code: "Space", ctrlKey: true });
    await waitFor(() =>
      expect(screen.getByText(/already in use/i)).toBeInTheDocument()
    );
    expect(props.onSave).toHaveBeenCalled();
    // Still displaying the live binding, not the rejected one.
    expect(screen.getByText("⌘")).toBeInTheDocument();
  });

  it("complains about a modifier-less chord instead of saving it", async () => {
    const props = setup();
    fireEvent.click(screen.getByLabelText(/Change the spotlight hotkey/i));
    fireEvent.keyDown(window, { code: "KeyA" });
    await waitFor(() =>
      expect(screen.getByText(/modifier/i)).toBeInTheDocument()
    );
    expect(props.onSave).not.toHaveBeenCalled();
  });

  it("offers Reset only when the hotkey differs from the platform default", () => {
    setup({ hotkey: "Cmd+Alt+A", defaultHotkey: "Cmd+Alt+A" });
    expect(screen.queryByText(/Reset/i)).not.toBeInTheDocument();
  });

  it("resets to the platform default on request", async () => {
    const props = setup({
      hotkey: "Ctrl+Shift+K",
      platform: WINDOWS,
      defaultHotkey: "Ctrl+Alt+A",
    });
    fireEvent.click(screen.getByText(/Reset/i));
    await waitFor(() =>
      expect(props.onSave).toHaveBeenCalledWith("Ctrl+Alt+A")
    );
  });

  it("does not re-save the chord that is already active, whatever the order", () => {
    const props = setup();
    fireEvent.click(screen.getByLabelText(/Change the spotlight hotkey/i));
    fireEvent.keyDown(window, { code: "KeyA", metaKey: true, altKey: true });
    expect(props.onSave).not.toHaveBeenCalled();
  });
});
