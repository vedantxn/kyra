import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConnectionsSheet, SetupFlow } from "./App";
import type { AiEngineStatus, GoogleConnectorStatus } from "./contracts";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

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
  onOpenSetup: vi.fn(),
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

  it("offers the guided setup from the disconnected settings state", () => {
    render(
      <ConnectionsSheet
        {...actions}
        status={status("disconnected")}
        ai={ai}
        busy={false}
        error=""
        aiError=""
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open guided setup" }));
    expect(actions.onOpenSetup).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Connect directly" })).toBeEnabled();
  });
});

describe("First-run setup", () => {
  const setupActions = {
    onConnect: vi.fn().mockResolvedValue(undefined),
    onFinish: vi.fn(),
    onExplore: vi.fn(),
    onClose: vi.fn(),
  };

  it("explains Google access before asking for authorization", () => {
    render(<SetupFlow {...setupActions} status={status("disconnected")} busy={false} error="" />);
    expect(screen.getByRole("heading", { name: "Bring your real day into one calm view." })).toBeInTheDocument();
    expect(screen.getByText("Gmail is read-only")).toBeInTheDocument();
    expect(screen.getByText("Your primary Calendar")).toBeInTheDocument();
    expect(screen.getByText("Protected on this Mac")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Connect Gmail & Calendar/ }));
    expect(setupActions.onConnect).toHaveBeenCalledOnce();
  });

  it("shows an honest wait state while Google authorization is open", () => {
    render(<SetupFlow {...setupActions} status={{ ...status("connecting"), accountEmail: undefined }} busy error="" />);
    expect(screen.getByRole("heading", { name: "Finish connecting in your browser." })).toBeInTheDocument();
    expect(screen.getByText("Authorization in progress")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close setup" })).toBeDisabled();
  });

  it("distinguishes first synchronization from browser authorization", () => {
    render(<SetupFlow {...setupActions} status={status("syncing")} busy error="" />);
    expect(screen.getByRole("heading", { name: "Bringing your day into focus." })).toBeInTheDocument();
    expect(screen.getByText("IMPORTING YOUR WORKSPACE")).toBeInTheDocument();
    expect(screen.getByText("test@example.com")).toBeInTheDocument();
  });

  it("recovers revoked Google access without losing setup context", () => {
    render(<SetupFlow {...setupActions} status={{ ...status("reconnect_required"), lastError: undefined }} busy={false} error="" />);
    expect(screen.getByRole("heading", { name: "Google did not connect." })).toBeInTheDocument();
    expect(screen.getByText(/Google access has expired/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Try again/ })).toBeEnabled();
  });

  it("shows synchronized counts before finishing", () => {
    render(<SetupFlow {...setupActions} status={status("connected")} busy={false} error="" />);
    expect(screen.getByRole("heading", { name: "Your real day is ready." })).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("4")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Open Kyra/ }));
    expect(setupActions.onFinish).toHaveBeenCalledOnce();
  });

  it("keeps configuration failures actionable and recoverable", async () => {
    const { rerender } = render(<SetupFlow {...setupActions} status={status("disconnected")} busy={false} error="" />);
    fireEvent.click(screen.getByRole("button", { name: /Connect Gmail & Calendar/ }));
    rerender(<SetupFlow {...setupActions} status={status("disconnected")} busy={false} error="Add KYRA_GOOGLE_CLIENT_ID to .env.local before connecting Google." />);
    expect(await screen.findByRole("heading", { name: "Google did not connect." })).toBeInTheDocument();
    expect(screen.getByText(/KYRA_GOOGLE_CLIENT_ID/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Use the sample day for now" }));
    expect(setupActions.onExplore).toHaveBeenCalledOnce();
  });
});
