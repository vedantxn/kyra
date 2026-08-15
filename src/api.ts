import { invoke } from "@tauri-apps/api/core";
import { calendarBlockSchema, dashboardSchema, openLoopSchema, type Dashboard, type LoopStatus, type OpenLoop } from "./contracts";
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
    return calendarBlockSchema.parse({ id: crypto.randomUUID(), title, startAt, endAt, kind: "execution", color: "#7bcaa2" });
  }
  return calendarBlockSchema.parse(await invoke("create_calendar_block", { input: { title, startAt, endAt } }));
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
