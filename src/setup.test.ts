import { describe, expect, it, vi } from "vitest";
import type { GoogleConnectorStatus } from "./contracts";
import { readSetupPreference, SETUP_PREFERENCE_KEY, shouldShowSetup, writeSetupPreference } from "./setup";

const disconnected: GoogleConnectorStatus = {
  state: "disconnected",
  gmailMessageCount: 0,
  calendarEventCount: 0,
};

describe("setup preference", () => {
  it("shows setup only for a genuinely new disconnected user", () => {
    expect(shouldShowSetup(disconnected, null)).toBe(true);
    expect(shouldShowSetup(disconnected, "skipped")).toBe(false);
    expect(shouldShowSetup({ ...disconnected, state: "connected" }, null)).toBe(false);
  });

  it("persists only recognized setup choices", () => {
    const storage = { getItem: vi.fn().mockReturnValue("completed"), setItem: vi.fn() };
    expect(readSetupPreference(storage)).toBe("completed");
    writeSetupPreference("skipped", storage);
    expect(storage.setItem).toHaveBeenCalledWith(SETUP_PREFERENCE_KEY, "skipped");
    storage.getItem.mockReturnValue("unexpected");
    expect(readSetupPreference(storage)).toBeNull();
  });

  it("falls back safely when browser storage is unavailable", () => {
    const broken = {
      getItem: vi.fn(() => { throw new Error("blocked"); }),
      setItem: vi.fn(() => { throw new Error("blocked"); }),
    };
    expect(readSetupPreference(broken)).toBeNull();
    expect(() => writeSetupPreference("completed", broken)).not.toThrow();
  });
});
