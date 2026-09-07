import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { usePlatform } from "./usePlatform";
import type { PlatformInfo } from "../types";

const WINDOWS_INFO: PlatformInfo = {
  os: "windows",
  os_name: "Windows",
  file_manager: "Explorer",
  shell_name: "PowerShell",
  shell_tool_name: "run_powershell",
  read_list_hint: "Access the list using: $files = Get-Content 'x'",
};

const mockGetPlatformInfo = vi.fn<() => Promise<PlatformInfo>>(
  async () => WINDOWS_INFO
);

vi.mock("../utils/tauri", () => ({
  getPlatformInfo: (...args: unknown[]) =>
    mockGetPlatformInfo(...(args as [])),
}));

describe("usePlatform", () => {
  beforeEach(() => {
    mockGetPlatformInfo.mockClear();
    mockGetPlatformInfo.mockResolvedValue(WINDOWS_INFO);
  });

  it("starts from a usable fallback before the backend answers", () => {
    const { result } = renderHook(() => usePlatform());
    // jsdom's user agent is not a Mac, so the non-Mac fallback applies. What
    // matters is that no field is undefined on the first render — labels and
    // the synthesized tool name are read immediately.
    expect(result.current.os).toBeTruthy();
    expect(result.current.file_manager).toBeTruthy();
    expect(result.current.shell_tool_name).toBeTruthy();
  });

  it("replaces the fallback with the backend's answer", async () => {
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => expect(result.current.shell_name).toBe("PowerShell"));
    expect(result.current.file_manager).toBe("Explorer");
    expect(result.current.shell_tool_name).toBe("run_powershell");
  });

  it("keeps the fallback when the backend call fails", async () => {
    mockGetPlatformInfo.mockRejectedValue(new Error("ipc down"));
    const { result } = renderHook(() => usePlatform());
    await act(async () => {
      await Promise.resolve();
    });
    // Degrades to labels rather than throwing or rendering blanks — the
    // backend still accepts either shell-tool spelling, so a stale name here
    // is harmless.
    expect(result.current.file_manager).toBeTruthy();
    expect(["run_shell", "run_powershell"]).toContain(
      result.current.shell_tool_name
    );
  });

  it("only asks the backend once per mount", async () => {
    const { result } = renderHook(() => usePlatform());
    await waitFor(() => expect(result.current.shell_name).toBe("PowerShell"));
    expect(mockGetPlatformInfo).toHaveBeenCalledTimes(1);
  });
});
