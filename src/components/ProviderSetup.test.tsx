import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ProviderSetup } from "./ProviderSetup";
import type { ProviderStatus } from "../types";

function status(over: Partial<ProviderStatus> = {}): ProviderStatus {
  return {
    ollama_reachable: false,
    ollama_model_count: 0,
    openrouter_set: false,
    anthropic_set: false,
    any_usable: false,
    ...over,
  };
}

function setup(over: Partial<Parameters<typeof ProviderSetup>[0]> = {}) {
  const props = {
    status: status(),
    onRecheck: vi.fn(),
    onSaveKey: vi.fn().mockResolvedValue(undefined),
    ollamaUrl: "http://localhost:11434",
    onSaveOllamaUrl: vi.fn().mockResolvedValue(undefined),
    onOpenSettings: vi.fn(),
    onError: vi.fn(),
    ...over,
  };
  render(<ProviderSetup {...props} />);
  return props;
}

describe("ProviderSetup — first-run onboarding", () => {
  it("offers both the local and cloud paths", () => {
    setup();
    expect(screen.getByText("Local")).toBeInTheDocument();
    expect(screen.getByText("Cloud")).toBeInTheDocument();
    expect(
      screen.getByLabelText("OpenRouter API key")
    ).toBeInTheDocument();
  });

  it("says Ollama is not detected when it's unreachable", () => {
    setup({ status: status({ ollama_reachable: false }) });
    // Naming the URL it probed matters: "not detected" alone reads as
    // "not installed" even when cdout is pointed at the wrong host.
    expect(
      screen.getByText(/No Ollama at http:\/\/localhost:11434/i)
    ).toBeInTheDocument();
    // Install link only shows in the not-detected state.
    expect(screen.getByText(/Get Ollama/i)).toBeInTheDocument();
  });

  it("shows the pull command when Ollama runs but has no models", () => {
    setup({
      status: status({ ollama_reachable: true, ollama_model_count: 0 }),
    });
    expect(screen.getByText(/Ollama is running/i)).toBeInTheDocument();
    expect(screen.getByText("ollama pull llama3.2")).toBeInTheDocument();
  });

  it("saves a remote Ollama URL from onboarding", async () => {
    const props = setup();
    const input = screen.getByLabelText("Ollama server URL");
    fireEvent.change(input, {
      target: { value: "http://192.168.1.50:11434" },
    });
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() =>
      expect(props.onSaveOllamaUrl).toHaveBeenCalledWith(
        "http://192.168.1.50:11434"
      )
    );
  });

  it("does not save an unchanged Ollama URL", () => {
    const props = setup();
    // The button starts disabled because the field is pre-filled with the
    // current URL — clicking it must not re-save the same value.
    fireEvent.click(screen.getByText("Connect"));
    expect(props.onSaveOllamaUrl).not.toHaveBeenCalled();
  });

  it("trims whitespace before saving the Ollama URL", async () => {
    const props = setup();
    fireEvent.change(screen.getByLabelText("Ollama server URL"), {
      target: { value: "  http://box.local:11434  " },
    });
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() =>
      expect(props.onSaveOllamaUrl).toHaveBeenCalledWith(
        "http://box.local:11434"
      )
    );
  });

  it("surfaces an error when the URL save fails", async () => {
    const props = setup({
      onSaveOllamaUrl: vi.fn().mockRejectedValue("bad url"),
    });
    fireEvent.change(screen.getByLabelText("Ollama server URL"), {
      target: { value: "ftp://nope" },
    });
    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => expect(props.onError).toHaveBeenCalled());
  });

  it("shows a checking state until the status resolves", () => {
    setup({ status: null });
    expect(screen.getByText(/Checking for Ollama/i)).toBeInTheDocument();
  });

  it("saves the entered OpenRouter key", async () => {
    const props = setup();
    const input = screen.getByLabelText("OpenRouter API key");
    fireEvent.change(input, { target: { value: "sk-or-v1-abc" } });
    fireEvent.click(screen.getByText(/Save key & start/i));
    await waitFor(() =>
      expect(props.onSaveKey).toHaveBeenCalledWith("sk-or-v1-abc")
    );
  });

  it("does not save an empty key", () => {
    const props = setup();
    fireEvent.click(screen.getByText(/Save key & start/i));
    expect(props.onSaveKey).not.toHaveBeenCalled();
  });

  it("re-checks providers on demand", () => {
    const props = setup();
    fireEvent.click(screen.getByRole("button", { name: /re-check/i }));
    expect(props.onRecheck).toHaveBeenCalled();
  });
});
