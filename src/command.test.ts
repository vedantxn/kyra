import { describe, expect, it } from "vitest";
import { parseCommand } from "./command";

describe("parseCommand", () => {
  it("parses a task without losing its text", () => {
    expect(parseCommand("/task send the edited video")).toEqual({ kind: "task", title: "send the edited video" });
  });

  it("parses a calendar block into a one-hour window", () => {
    const result = parseCommand("/cal standup 9am", new Date("2026-08-15T00:00:00.000Z"));
    expect(result.kind).toBe("calendar");
    if (result.kind === "calendar") expect(new Date(result.endAt).getTime() - new Date(result.startAt).getTime()).toBe(3_600_000);
  });

  it("rejects commands without an explicit action", () => {
    expect(parseCommand("remember something")).toEqual({ kind: "unknown", message: "Start with /task or /cal." });
  });
});
