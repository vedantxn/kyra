import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConnectionsSheet } from "./App";
import type { AiEngineStatus, GoogleConnectorStatus } from "./contracts";

afterEach(cleanup);

const actions = {
  onClose: vi.fn(),
  onConnect: vi.fn(),
  onSync: vi.fn(),
  onDisconnect: vi.fn(),
  onSaveAi: vi.fn(),
  onActivateAi: vi.fn(),
  onRunAi: vi.fn(),
  onClearAi: vi.fn(),
  onDiscoverOllama: vi.fn().mockResolvedValue([]),
};

const ai: AiEngineStatus = {
  state: "disconnected",
  queuedJobs: 0,
  runningJobs: 0,
  failedJobs: 0,
  reviewCount: 0,
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
        ai={ai}
        busy={false}
        error=""
        aiError=""
      />,
    );
    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("shows synchronized counts and recovery controls", () => {
    render(
      <ConnectionsSheet
        {...actions}
        status={status("reconnect_required")}
        ai={ai}
        busy={false}
        error=""
        aiError=""
      />,
    );
    expect(screen.getByText("12 messages")).toBeInTheDocument();
    expect(screen.getByText("4 events")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Disconnect/ })).toBeEnabled();
  });

  it("renders activated model state without exposing a saved key", () => {
    render(
      <ConnectionsSheet
        {...actions}
        status={status("connected")}
        ai={{ ...ai, state: "ready", provider: "openai", requestedModel: "gpt-test", activatedModel: "gpt-test-2026", queuedJobs: 3, reviewCount: 2 }}
        busy={false}
        error=""
        aiError=""
      />,
    );
    expect(screen.getByText("gpt-test-2026")).toBeInTheDocument();
    expect(screen.getByText("3 queued · 0 failed")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Leave blank to keep saved key")).toHaveValue("");
    expect(screen.getByRole("button", { name: /Test & activate/ })).toBeEnabled();
  });
});
