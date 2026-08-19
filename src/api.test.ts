import { afterEach, describe, expect, it, vi } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  executeAiCommand,
  connectGoogle,
  disconnectGoogle,
  getAiEngineStatus,
  getGoogleConnectorStatus,
  listAiActivity,
  listAiReviews,
  mutateGoogleCalendar,
  retryAiJob,
  revertAiAction,
  saveAiProviderConfig,
  syncGoogleNow,
  testAiProvider,
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

describe("native AI engine API", () => {
  it("keeps secrets write-only while validating every typed response", async () => {
    const calls: Array<{ command: string; payload?: unknown }> = [];
    mockIPC((command, payload) => {
      calls.push({ command, payload });
      if (command === "test_ai_provider") {
        return { fingerprint: "fp", provider: "openai", requestedModel: "gpt-test", resolvedModel: "gpt-test-2026", casesRun: 12, schemaValidity: 1, evidenceValidity: 1, requiredActionCoverage: 1, confirmedMeetingRecall: 1, unauthorizedActions: 0, ambiguousCalendarActions: 0, maxLatencyMs: 10, passed: true };
      }
      if (command === "execute_ai_command") return { kind: "executed", actionIds: ["action-1"] };
      if (command === "list_ai_reviews") return [{ id: "review-1", kind: "task_ambiguous", title: "Check this", summary: "Identity is ambiguous.", evidence: ["exact quote"], irreversibleEffects: [], createdAt: "2026-08-18T00:00:00Z" }];
      if (command === "list_ai_activity") return [{ id: "job-1", kind: "job", status: "failed", title: "Email analysis needs attention", detail: "Safe error", canRevert: false, compensation: false, irreversibleEffects: [], createdAt: "2026-08-18T00:00:00Z" }];
      if (command === "revert_ai_action") return { actionId: "action-1", status: "reverted", message: "Restored." };
      return { state: "ready", provider: "openai", requestedModel: "gpt-test", activatedModel: "gpt-test-2026", queuedJobs: 1, runningJobs: 0, failedJobs: 0, reviewCount: 1 };
    });

    const saved = await saveAiProviderConfig({ provider: "openai", model: "gpt-test", apiKey: "write-only-secret" });
    expect(saved).not.toHaveProperty("apiKey");
    expect((await getAiEngineStatus()).state).toBe("ready");
    expect((await testAiProvider()).passed).toBe(true);
    expect(await executeAiCommand("create the report task")).toEqual({ kind: "executed", actionIds: ["action-1"] });
    expect((await listAiReviews())[0].id).toBe("review-1");
    expect((await listAiActivity())[0].kind).toBe("job");
    expect((await retryAiJob("job-1")).queuedJobs).toBe(1);
    expect((await revertAiAction("action-1")).status).toBe("reverted");
    expect(calls.find((call) => call.command === "save_ai_provider_config")?.payload).toEqual({ input: { provider: "openai", model: "gpt-test", apiKey: "write-only-secret" } });
  });
});
