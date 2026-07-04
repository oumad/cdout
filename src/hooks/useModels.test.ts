import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { StrictMode, createElement, type ReactNode } from "react";
import { useModels } from "./useModels";

// Mock the Tauri IPC surface with a mutable slug list so we can drive
// stale-slug reconciliation scenarios.
const mockGetOllamaModels = vi.fn<() => Promise<string[]>>(async () => [
  "ollama:llama3",
  "anthropic/claude-opus-4.7",
]);
const mockGetSelectedModel = vi.fn<() => Promise<string | null>>(
  async () => "ollama:llama3"
);
const mockSetSelectedModel = vi.fn<(m: string) => Promise<void>>(
  async () => undefined
);

vi.mock("../utils/tauri", () => ({
  getOllamaModels: (...args: unknown[]) => mockGetOllamaModels(...(args as [])),
  getSelectedModel: (...args: unknown[]) => mockGetSelectedModel(...(args as [])),
  setSelectedModel: (m: string) => mockSetSelectedModel(m),
}));

const strictWrapper = ({ children }: { children: ReactNode }) =>
  createElement(StrictMode, null, children);

async function flushAll() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("useModels — port-exhaustion regression", () => {
  beforeEach(() => {
    mockGetOllamaModels.mockClear();
    mockGetSelectedModel.mockClear();
    mockSetSelectedModel.mockClear();
    mockGetOllamaModels.mockImplementation(async () => [
      "ollama:llama3",
      "anthropic/claude-opus-4.7",
    ]);
    mockGetSelectedModel.mockImplementation(async () => "ollama:llama3");
  });

  /**
   * The bug we're regression-testing: the parent (App.tsx) passes a fresh
   * `{ onSwap: ... }` object every render. If useModels put `opts` in
   * fetchModels' useCallback deps, fetchModels identity would change every
   * render, the initial-fetch useEffect would re-fire every render, and
   * during streaming this hit Ollama ~80x/second on Windows.
   *
   * The fix uses a ref for onSwap and empty deps on fetchModels. Verify:
   *   1) A parent re-render (opts identity change) does NOT trigger a new
   *      getOllamaModels() call.
   *   2) fetchModels identity is stable across parent re-renders.
   */
  it("does not re-fetch on every parent render when opts identity changes", async () => {
    const { result, rerender } = renderHook(
      ({ onSwap }: { onSwap?: (a: string, b: string) => void }) =>
        useModels("http://localhost:11434", { onSwap }),
      { initialProps: { onSwap: () => {} } }
    );

    // Flush the initial fetch.
    await flushAll();

    const callsAfterInitial = mockGetOllamaModels.mock.calls.length;
    expect(callsAfterInitial).toBeGreaterThan(0);

    // Simulate 20 parent re-renders — each with a fresh onSwap function
    // literal, mirroring what App.tsx does today.
    const firstFetch = result.current.fetchModels;
    for (let i = 0; i < 20; i++) {
      rerender({ onSwap: () => {} });
    }
    await flushAll();

    const callsAfterRerenders = mockGetOllamaModels.mock.calls.length;

    // The critical assertion: parent re-renders MUST NOT trigger new fetches.
    expect(callsAfterRerenders).toBe(callsAfterInitial);
    // And fetchModels identity must be stable.
    expect(result.current.fetchModels).toBe(firstFetch);
  });

  it("collapses overlapping fetchModels calls via the inflight guard", async () => {
    const { result } = renderHook(() =>
      useModels("http://localhost:11434", { onSwap: () => {} })
    );

    // Flush the initial mount fetch first.
    await flushAll();
    const initialCount = mockGetOllamaModels.mock.calls.length;

    // Fire 10 manual refreshes back-to-back before yielding to the
    // microtask queue — the guard should collapse them to at most one.
    await act(async () => {
      await Promise.all([
        result.current.fetchModels(),
        result.current.fetchModels(),
        result.current.fetchModels(),
        result.current.fetchModels(),
        result.current.fetchModels(),
      ]);
    });

    const afterBurst = mockGetOllamaModels.mock.calls.length;
    // The 5 concurrent calls collapse to a single network hit.
    expect(afterBurst - initialCount).toBeLessThanOrEqual(1);
  });

  /**
   * React.StrictMode intentionally double-invokes updater functions in dev
   * to surface impurity. If setModelName's updater still called
   * `api.setSelectedModel` or `onSwap` from inside the updater (the old
   * bug), those side effects would fire twice on every stale-slug swap.
   *
   * We prove the fix by:
   *   1) starting from a settled state,
   *   2) swapping the mocked model list so the current selection is stale,
   *   3) manually triggering fetchModels() under StrictMode,
   *   4) asserting onSwap AND setSelectedModel each fire exactly once.
   */
  it("under StrictMode, a stale-slug swap fires setSelectedModel exactly once", async () => {
    // Start from a healthy state.
    mockGetSelectedModel.mockImplementation(async () => "ollama:llama3");
    mockGetOllamaModels.mockImplementation(async () => ["ollama:llama3"]);

    const swaps: Array<{ prev: string; next: string }> = [];
    const { result } = renderHook(
      () =>
        useModels("http://localhost:11434", {
          onSwap: (prev, next) => swaps.push({ prev, next }),
        }),
      { wrapper: strictWrapper }
    );

    // Let the initial fetch settle so modelName is "ollama:llama3".
    await flushAll();
    await flushAll();

    // Baseline the mock counters — we only care about calls AFTER the swap.
    mockSetSelectedModel.mockClear();
    const swapsBefore = swaps.length;

    // Now the model list changes — the current selection is stale.
    mockGetOllamaModels.mockImplementation(async () => [
      "anthropic/claude-opus-4.7",
    ]);
    await act(async () => {
      await result.current.fetchModels();
    });
    await flushAll();

    const swapDelta = swaps.length - swapsBefore;
    // The critical assertion: the swap side effect fires EXACTLY once even
    // though StrictMode double-invokes the updater.
    expect(swapDelta).toBe(1);
    expect(swaps[swapsBefore].prev).toBe("ollama:llama3");
    expect(swaps[swapsBefore].next).toBe("anthropic/claude-opus-4.7");

    // Same guarantee for the IPC persist call — exactly one write per swap.
    const persistCalls = mockSetSelectedModel.mock.calls.filter(
      ([m]) => m === "anthropic/claude-opus-4.7"
    );
    expect(persistCalls.length).toBe(1);
  });

  it("under StrictMode, fetchModels identity is stable across parent re-renders", async () => {
    const { result, rerender } = renderHook(
      ({ onSwap }: { onSwap?: (a: string, b: string) => void }) =>
        useModels("http://localhost:11434", { onSwap }),
      {
        initialProps: { onSwap: () => {} },
        wrapper: strictWrapper,
      }
    );

    await flushAll();
    const first = result.current.fetchModels;

    for (let i = 0; i < 10; i++) {
      rerender({ onSwap: () => {} });
    }
    await flushAll();

    expect(result.current.fetchModels).toBe(first);
  });
});
