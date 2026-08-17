import { afterEach, describe, expect, it, vi } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  connectGoogle,
  disconnectGoogle,
  getGoogleConnectorStatus,
  mutateGoogleCalendar,
  syncGoogleNow,
} from "./api";

afterEach(() => clearMocks());

const connected = {
  state: "connected",
  accountEmail: "test@example.com",
  lastSyncAt: "2026-08-17T12:00:00Z",
  nextSyncAt: "2026-08-17T12:05:00Z",
  gmailMessageCount: 24,
  calendarEventCount: 7,
};

describe("native Google connector API", () => {
  it("keeps browser fixture mode disconnected", async () => {
    expect(await getGoogleConnectorStatus()).toEqual({
      state: "disconnected",
      gmailMessageCount: 0,
      calendarEventCount: 0,
    });
  });

  it("invokes connect, status, sync, and disconnect commands", async () => {
    const commands: string[] = [];
    mockIPC((command) => {
      commands.push(command);
      if (command === "sync_google_now") {
        return { gmailMessageCount: 24, calendarEventCount: 7, completedAt: "2026-08-17T12:00:00Z" };
      }
      if (command === "disconnect_google") {
        return { state: "disconnected", gmailMessageCount: 0, calendarEventCount: 0 };
      }
      return connected;
    });

    expect((await getGoogleConnectorStatus()).state).toBe("connected");
    expect((await connectGoogle()).accountEmail).toBe("test@example.com");
    expect((await syncGoogleNow()).gmailMessageCount).toBe(24);
    expect((await disconnectGoogle()).state).toBe("disconnected");
    expect(commands).toEqual([
      "get_google_connector_status",
      "connect_google",
      "sync_google_now",
      "disconnect_google",
    ]);
  });

  it("passes a typed calendar mutation to Tauri", async () => {
    const handler = vi.fn((_command: string, payload?: unknown) => {
      const input = (payload as { input?: { operationId: string } })?.input;
      if (!input) throw new Error("Missing mutation input");
      return { operationId: input.operationId, event: null, deleted: false };
    });
    mockIPC(handler);
    const input = {
      action: "create" as const,
      operationId: "op-1",
      event: {
        title: "Design review",
        when: {
          kind: "timed" as const,
          startAt: "2026-08-17T10:00:00+05:30",
          endAt: "2026-08-17T11:00:00+05:30",
          timeZone: "Asia/Kolkata",
        },
        sendUpdates: "all" as const,
      },
    };
    expect((await mutateGoogleCalendar(input)).operationId).toBe("op-1");
    expect(handler).toHaveBeenCalledWith("mutate_google_calendar", { input });
  });
});
