import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { ChatMessage } from "./ChatMessage";
import type { Message } from "../types";

function userMsg(content: string, synthetic = false): Message {
  return { role: "user", content, synthetic };
}

describe("ChatMessage — synthetic vs real user messages", () => {
  it("renders a real user message as the user's input bubble", () => {
    render(
      <ChatMessage
        message={userMsg("rename my photos")}
        index={0}
        isExpanded={false}
        onToggle={vi.fn()}
      />
    );
    expect(screen.getByText("rename my photos")).toBeInTheDocument();
    // Real user messages do NOT carry the "cdout internal" badge.
    expect(screen.queryByText(/cdout internal/i)).not.toBeInTheDocument();
  });

  it("renders a synthetic continuation nudge with the cdout-internal badge", () => {
    render(
      <ChatMessage
        message={userMsg(
          "If the task is complete, reply with exactly 'Task Complete' and stop.",
          true
        )}
        index={1}
        isExpanded={false}
        onToggle={vi.fn()}
      />
    );
    expect(screen.getByText(/cdout internal/i)).toBeInTheDocument();
    // The continuation text itself is still visible to the user.
    expect(
      screen.getByText(/If the task is complete/)
    ).toBeInTheDocument();
  });

  it("renders a synthetic loop-detected note distinctly from a user message", () => {
    render(
      <ChatMessage
        message={userMsg(
          "[Loop detected] Same tool call has appeared 3 times — stop, reassess.",
          true
        )}
        index={2}
        isExpanded={false}
        onToggle={vi.fn()}
      />
    );
    expect(screen.getByText(/cdout internal/i)).toBeInTheDocument();
    expect(screen.getByText(/\[Loop detected\]/)).toBeInTheDocument();
  });

  it("renders a synthetic interrupt note distinctly from a user message", () => {
    render(
      <ChatMessage
        message={userMsg(
          "[Interrupted by user] The user stopped this action.",
          true
        )}
        index={3}
        isExpanded={false}
        onToggle={vi.fn()}
      />
    );
    expect(screen.getByText(/cdout internal/i)).toBeInTheDocument();
  });
});
