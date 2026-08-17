import { invoke } from "@tauri-apps/api/core";
import {
  calendarBlockSchema,
  calendarMutationResultSchema,
  dashboardSchema,
  googleConnectorStatusSchema,
  googleSyncSummarySchema,
  openLoopSchema,
  type CalendarMutationInput,
  type Dashboard,
  type GoogleConnectorStatus,
  type LoopStatus,
  type OpenLoop,
} from "./contracts";
import { demoDashboard } from "./demo";

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function getDashboard(): Promise<Dashboard> {
  if (!isTauri()) {
    return dashboardSchema.parse(demoDashboard);
  }

  return dashboardSchema.parse(await invoke("get_dashboard"));
}

export async function createTask(title: string): Promise<OpenLoop> {
  if (!isTauri()) {
    return openLoopSchema.parse({
      id: crypto.randomUUID(),
      title,
      summary: "Added directly by you.",
      owner: "me",
      status: "open",
      priority: 50,
      dueAt: null,
      version: 1,
      evidence: [],
    });
  }

  return openLoopSchema.parse(await invoke("create_task", { input: { title } }));
}

export async function createCalendarBlock(title: string, startAt: string, endAt: string) {
  if (!isTauri()) {
    return calendarBlockSchema.parse({ id: crypto.randomUUID(), title, startAt, endAt, kind: "execution", color: "#8ca481" });
  }
  return calendarBlockSchema.parse(await invoke("create_calendar_block", { input: { title, startAt, endAt } }));
}

const disconnectedStatus: GoogleConnectorStatus = {
  state: "disconnected",
  gmailMessageCount: 0,
  calendarEventCount: 0,
};

export async function getGoogleConnectorStatus(): Promise<GoogleConnectorStatus> {
  if (!isTauri()) return googleConnectorStatusSchema.parse(disconnectedStatus);
  return googleConnectorStatusSchema.parse(await invoke("get_google_connector_status"));
}

export async function connectGoogle(): Promise<GoogleConnectorStatus> {
  if (!isTauri()) throw new Error("Google connection is only available in the native Kyra app.");
  return googleConnectorStatusSchema.parse(await invoke("connect_google"));
}

export async function disconnectGoogle(): Promise<GoogleConnectorStatus> {
  if (!isTauri()) return googleConnectorStatusSchema.parse(disconnectedStatus);
  return googleConnectorStatusSchema.parse(await invoke("disconnect_google"));
}

export async function syncGoogleNow() {
  if (!isTauri()) throw new Error("Google connection is only available in the native Kyra app.");
  return googleSyncSummarySchema.parse(await invoke("sync_google_now"));
}

export async function mutateGoogleCalendar(input: CalendarMutationInput) {
  if (!isTauri()) throw new Error("Google Calendar is only available in the native Kyra app.");
  return calendarMutationResultSchema.parse(await invoke("mutate_google_calendar", { input }));
}

export async function setLoopStatus(id: string, status: LoopStatus, expectedVersion: number): Promise<OpenLoop> {
  if (!isTauri()) {
    const loop = demoDashboard.openLoops.find((item) => item.id === id);
    if (!loop) throw new Error("Open loop not found");
    return openLoopSchema.parse({ ...loop, status, version: expectedVersion + 1 });
  }
  return openLoopSchema.parse(
    await invoke("set_loop_status", { input: { id, status, expectedVersion } }),
  );
}

export async function hideOverlay() {
  if (isTauri()) await invoke("hide_overlay");
}
