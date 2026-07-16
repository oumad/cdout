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
    expect(screen.getByText(/Ollama not detected/i)).toBeInTheDocument();
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
