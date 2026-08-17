import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConnectionsSheet } from "./App";
import type { GoogleConnectorStatus } from "./contracts";

afterEach(cleanup);

const actions = {
  onClose: vi.fn(),
  onConnect: vi.fn(),
  onSync: vi.fn(),
  onDisconnect: vi.fn(),
};

function status(state: GoogleConnectorStatus["state"]): GoogleConnectorStatus {
  return {
    state,
    accountEmail: state === "disconnected" ? undefined : "test@example.com",
    gmailMessageCount: 12,
    calendarEventCount: 4,
    lastSyncAt: "2026-08-17T12:00:00Z",
    lastError: state === "error" ? "Kyra could not reach Google. Cached data is still available." : undefined,
  };
}

describe("Connections sheet", () => {
  it.each([
    ["disconnected", "Not connected"],
    ["connecting", "Waiting for Google"],
    ["syncing", "Synchronizing"],
    ["connected", "Connected"],
    ["reconnect_required", "Reconnect required"],
    ["error", "Retry scheduled"],
  ] as const)("renders the %s state", (stateName, label) => {
    render(
      <ConnectionsSheet
        {...actions}
        status={status(stateName)}
        busy={false}
        error=""
      />,
    );
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("shows synchronized counts and recovery controls", () => {
    render(
      <ConnectionsSheet
        {...actions}
        status={status("reconnect_required")}
        busy={false}
        error=""
      />,
    );
    expect(screen.getByText("12 messages")).toBeInTheDocument();
    expect(screen.getByText("4 events")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Disconnect/ })).toBeEnabled();
  });
});
