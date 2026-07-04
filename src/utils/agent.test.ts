import { describe, it, expect } from "vitest";
import { isTaskComplete } from "./agent";

describe("isTaskComplete", () => {
  it("returns true for explicit completion text", () => {
    expect(isTaskComplete("Task Complete")).toBe(true);
    expect(isTaskComplete("All done.")).toBe(true);
    expect(isTaskComplete("Files have been processed.")).toBe(true);
  });

  it("returns true for completed successfully variant", () => {
    expect(isTaskComplete("Completed successfully.")).toBe(true);
  });

  it("returns false for in-progress text", () => {
    expect(isTaskComplete("Running the command now...")).toBe(false);
    expect(isTaskComplete("Let me check the output first.")).toBe(false);
  });

  it("returns false for empty / nothing-to-say", () => {
    expect(isTaskComplete("")).toBe(false);
    expect(isTaskComplete("   ")).toBe(false);
  });

  it("is case insensitive", () => {
    expect(isTaskComplete("TASK COMPLETE")).toBe(true);
    expect(isTaskComplete("task complete")).toBe(true);
  });
});
