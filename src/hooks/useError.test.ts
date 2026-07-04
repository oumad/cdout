import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useError } from "./useError";

describe("useError", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts with no error", () => {
    const { result } = renderHook(() => useError());
    expect(result.current.error).toBe("");
  });

  it("showError sets the message", () => {
    const { result } = renderHook(() => useError());
    act(() => {
      result.current.showError("oops");
    });
    expect(result.current.error).toBe("oops");
  });

  it("clearError clears the message", () => {
    const { result } = renderHook(() => useError());
    act(() => {
      result.current.showError("oops");
    });
    act(() => {
      result.current.clearError();
    });
    expect(result.current.error).toBe("");
  });

  it("auto-dismisses after the timeout", () => {
    const { result } = renderHook(() => useError());
    act(() => {
      result.current.showError("transient");
    });
    expect(result.current.error).toBe("transient");
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(result.current.error).toBe("");
  });
});
