// Vitest global setup. Stub out the @tauri-apps/api/core invoke + Channel
// surface so component tests don't try to bridge into a real Tauri runtime.

import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => undefined),
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "main",
    onFocusChanged: async () => () => {},
    hide: vi.fn(),
    show: vi.fn(),
    unminimize: vi.fn(),
    setFocus: vi.fn(),
    cursorPosition: vi.fn(),
    availableMonitors: vi.fn(),
    outerSize: vi.fn(),
    setPosition: vi.fn(),
  }),
}));
