import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { FileAccessDiagnostics } from "./FileAccessDiagnostics";
import type { ExplorerDebugInfo, PlatformInfo } from "../types";

const mockGetExplorerDebug = vi.fn<() => Promise<ExplorerDebugInfo>>();
const mockOpenSettings = vi.fn<() => Promise<void>>();

vi.mock("../utils/tauri", () => ({
  getExplorerDebug: () => mockGetExplorerDebug(),
  openFileAccessSettings: () => mockOpenSettings(),
}));

const MAC: PlatformInfo = {
  os: "macos",
  os_name: "macOS",
  file_manager: "Finder",
  shell_name: "zsh",
  shell_tool_name: "run_shell",
  read_list_hint: "",
};

const WINDOWS: PlatformInfo = {
  os: "windows",
  os_name: "Windows",
  file_manager: "Explorer",
  shell_name: "PowerShell",
  shell_tool_name: "run_powershell",
  read_list_hint: "",
};

describe("FileAccessDiagnostics", () => {
  beforeEach(() => {
    mockGetExplorerDebug.mockReset();
    mockOpenSettings.mockReset();
    mockOpenSettings.mockResolvedValue(undefined);
  });

  it("names the denial and offers the fix when Automation is refused", async () => {
    mockGetExplorerDebug.mockResolvedValue({
      os: "macos",
      automation_authorized: false,
      script_error: "not allowed to control Finder",
    });
    render(<FileAccessDiagnostics platform={MAC} />);

    await waitFor(() =>
      expect(screen.getByText(/cdout cannot read Finder/i)).toBeInTheDocument()
    );
    // The whole point: a one-click route to the checkbox.
    fireEvent.click(screen.getByRole("button", { name: /Open Settings/i }));
    await waitFor(() => expect(mockOpenSettings).toHaveBeenCalled());
  });

  it("treats a failed debug call as a denial on macOS", async () => {
    // The backend surfaces the TCC refusal as an Err, so the error path and
    // the unauthorized path must land in the same place.
    mockGetExplorerDebug.mockRejectedValue("not allowed to control Finder");
    render(<FileAccessDiagnostics platform={MAC} />);
    await waitFor(() =>
      expect(screen.getByText(/cdout cannot read Finder/i)).toBeInTheDocument()
    );
  });

  it("reports the folder it can see when access works", async () => {
    mockGetExplorerDebug.mockResolvedValue({
      os: "macos",
      automation_authorized: true,
      resolved_path: "/Users/me/Movies",
      selected_count: 3,
    });
    render(<FileAccessDiagnostics platform={MAC} />);
    await waitFor(() =>
      expect(screen.getByText(/Finder access is working/i)).toBeInTheDocument()
    );
    expect(screen.getByText("/Users/me/Movies")).toBeInTheDocument();
    expect(screen.getByText(/3 file\(s\) selected/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Open Settings/i })
    ).not.toBeInTheDocument();
  });

  it("distinguishes 'no window open' from 'not allowed'", async () => {
    mockGetExplorerDebug.mockResolvedValue({
      os: "macos",
      automation_authorized: true,
      finder_window_count: 0,
      resolved_path: "",
    });
    render(<FileAccessDiagnostics platform={MAC} />);
    await waitFor(() =>
      expect(screen.getByText(/No Finder window is open/i)).toBeInTheDocument()
    );
  });

  it("never offers a permission fix on Windows, which has no grant", async () => {
    mockGetExplorerDebug.mockResolvedValue({
      os: "windows",
      is_explorer_window: true,
      target_explorer_title: "Movies",
    });
    render(<FileAccessDiagnostics platform={WINDOWS} />);
    await waitFor(() =>
      expect(
        screen.getByText(/Explorer access is working/i)
      ).toBeInTheDocument()
    );
    expect(
      screen.queryByRole("button", { name: /Open Settings/i })
    ).not.toBeInTheDocument();
  });

  it("re-checks on demand", async () => {
    mockGetExplorerDebug.mockResolvedValue({ automation_authorized: true });
    render(<FileAccessDiagnostics platform={MAC} />);
    await waitFor(() => expect(mockGetExplorerDebug).toHaveBeenCalledTimes(1));
    // By role, not text: the body copy also contains "re-check".
    fireEvent.click(screen.getByRole("button", { name: /Re-check/i }));
    await waitFor(() => expect(mockGetExplorerDebug).toHaveBeenCalledTimes(2));
  });

  it("surfaces the raw payload for support", async () => {
    mockGetExplorerDebug.mockResolvedValue({
      automation_authorized: true,
      finder_window_count: 2,
    });
    render(<FileAccessDiagnostics platform={MAC} />);
    await waitFor(() =>
      expect(screen.getByText(/Raw diagnostics/i)).toBeInTheDocument()
    );
    expect(screen.getByText(/finder_window_count/)).toBeInTheDocument();
  });
});
