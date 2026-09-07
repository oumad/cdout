import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { CommandApproval } from "./CommandApproval";
import type { CommandRisk } from "../types";

function setup(over: Partial<Parameters<typeof CommandApproval>[0]> = {}) {
  const props = {
    command: "ffprobe -v error clip.mov",
    onChange: vi.fn(),
    onApprove: vi.fn(),
    onReject: vi.fn(),
    onDismiss: vi.fn(),
    isExecuting: false,
    isProcessing: false,
    ...over,
  };
  render(<CommandApproval {...props} />);
  return props;
}

describe("CommandApproval", () => {
  it("says plainly when a proposal is destructive", () => {
    // The one case that must never be skimmed past. This label is shown even
    // to users who opted into running everything unattended, because that
    // guard is not disableable.
    setup({ command: "sudo rm -rf /", risk: "dangerous" as CommandRisk });
    expect(screen.getByText(/Destructive — read it/i)).toBeInTheDocument();
  });

  it("distinguishes a write from a read", () => {
    setup({ command: "ffmpeg -i a.mov b.mp4", risk: "mutating" });
    expect(screen.getByText(/Writes files/i)).toBeInTheDocument();
    expect(screen.queryByText(/Destructive/i)).not.toBeInTheDocument();
  });

  it("labels a read-only proposal so it can be approved at a glance", () => {
    // Only reachable in "ask every time" mode; saying so is what lets the
    // user click through without reading the command closely.
    setup({ risk: "read_only" });
    expect(screen.getByText(/Read-only/i)).toBeInTheDocument();
  });

  it("shows no badge when the risk is unknown", () => {
    setup({ risk: null });
    expect(screen.queryByText(/Read-only/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Writes files/i)).not.toBeInTheDocument();
  });

  it("still approves for this command only, or for the rest of the run", () => {
    const props = setup();
    fireEvent.click(screen.getByRole("button", { name: /^Execute/i }));
    expect(props.onApprove).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole("button", { name: /^All/i }));
    expect(props.onApprove).toHaveBeenCalledWith(true);
  });

  it("keeps the command editable before approval", () => {
    const props = setup();
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "ls -la" },
    });
    expect(props.onChange).toHaveBeenCalledWith("ls -la");
  });
});
