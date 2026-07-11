import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QuestionApproval } from "./QuestionApproval";
import type { PendingQuestion } from "../types";

function makeQuestion(overrides: Partial<PendingQuestion["data"]> = {}): PendingQuestion {
  return {
    tool_call_id: "tu_1",
    data: {
      question: "Which format?",
      options: [{ label: "mp4" }, { label: "mov" }],
      multi_select: false,
      ...overrides,
    },
  };
}

describe("QuestionApproval", () => {
  it("renders the question and options", () => {
    render(
      <QuestionApproval
        question={makeQuestion()}
        onAnswer={vi.fn()}
        onDismiss={vi.fn()}
      />
    );
    expect(screen.getByText("Which format?")).toBeInTheDocument();
    expect(screen.getByText("mp4")).toBeInTheDocument();
    expect(screen.getByText("mov")).toBeInTheDocument();
  });

  it("submit is disabled until an option is picked", () => {
    render(
      <QuestionApproval
        question={makeQuestion()}
        onAnswer={vi.fn()}
        onDismiss={vi.fn()}
      />
    );
    const submit = screen.getByText("Send Answer").closest("button")!;
    expect(submit).toBeDisabled();
  });

  it("submits the picked option label", () => {
    const onAnswer = vi.fn();
    render(
      <QuestionApproval
        question={makeQuestion()}
        onAnswer={onAnswer}
        onDismiss={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText("mp4"));
    const submit = screen.getByText("Send Answer").closest("button")!;
    fireEvent.click(submit);
    expect(onAnswer).toHaveBeenCalledWith("mp4");
  });

  it("multi-select picks accumulate and submit as comma-joined", () => {
    const onAnswer = vi.fn();
    render(
      <QuestionApproval
        question={makeQuestion({ multi_select: true })}
        onAnswer={onAnswer}
        onDismiss={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText("mp4"));
    fireEvent.click(screen.getByText("mov"));
    fireEvent.click(screen.getByText("Send Answer").closest("button")!);
    expect(onAnswer).toHaveBeenCalledWith("mp4, mov");
  });

  it("Other (custom answer) submits the typed text", () => {
    const onAnswer = vi.fn();
    render(
      <QuestionApproval
        question={makeQuestion()}
        onAnswer={onAnswer}
        onDismiss={vi.fn()}
      />
    );
    fireEvent.click(screen.getByText(/Other/));
    const textarea = screen.getByPlaceholderText("Type your answer...");
    fireEvent.change(textarea, { target: { value: "webm at 1080p" } });
    fireEvent.click(screen.getByText("Send Answer").closest("button")!);
    expect(onAnswer).toHaveBeenCalledWith("webm at 1080p");
  });

  it("dismiss calls onDismiss", () => {
    const onDismiss = vi.fn();
    render(
      <QuestionApproval
        question={makeQuestion()}
        onAnswer={vi.fn()}
        onDismiss={onDismiss}
      />
    );
    fireEvent.click(screen.getByText("Dismiss").closest("button")!);
    expect(onDismiss).toHaveBeenCalled();
  });
});
