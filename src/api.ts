import { invoke } from "@tauri-apps/api/core";
import {
  activationReportSchema,
  aiActivitySchema,
  aiCommandResultSchema,
  aiEngineStatusSchema,
  aiReviewSchema,
  aiCommandInputSchema,
  calendarBlockSchema,
  calendarMutationResultSchema,
  dashboardSchema,
  googleConnectorStatusSchema,
  googleSyncSummarySchema,
  ollamaModelSchema,
  openLoopSchema,
  resolveAiReviewInputSchema,
  revertAiActionResultSchema,
  saveAiProviderConfigSchema,
  type AiActivity,
  type AiCommandResult,
  type AiEngineStatus,
  type AiProvider,
  type AiReview,
  type CalendarMutationInput,
  type Dashboard,
  type GoogleConnectorStatus,
  type LoopStatus,
  type OpenLoop,
  type SaveAiProviderConfigInput,
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

let fixtureAiStatus: AiEngineStatus = {
  state: "disconnected",
  queuedJobs: 0,
  runningJobs: 0,
  failedJobs: 0,
  reviewCount: 0,
};

export async function getAiEngineStatus(): Promise<AiEngineStatus> {
  if (!isTauri()) return aiEngineStatusSchema.parse(fixtureAiStatus);
  return aiEngineStatusSchema.parse(await invoke("get_ai_engine_status"));
}

export async function saveAiProviderConfig(input: SaveAiProviderConfigInput): Promise<AiEngineStatus> {
  const validated = saveAiProviderConfigSchema.parse(input);
  if (!isTauri()) {
    fixtureAiStatus = { ...fixtureAiStatus, state: "disconnected", provider: validated.provider, requestedModel: validated.model };
    return aiEngineStatusSchema.parse(fixtureAiStatus);
  }
  return aiEngineStatusSchema.parse(await invoke("save_ai_provider_config", { input: validated }));
}

export async function clearAiProvider(provider: AiProvider): Promise<AiEngineStatus> {
  if (!isTauri()) {
    fixtureAiStatus = { state: "disconnected", queuedJobs: 0, runningJobs: 0, failedJobs: 0, reviewCount: 0 };
    return aiEngineStatusSchema.parse(fixtureAiStatus);
  }
  return aiEngineStatusSchema.parse(await invoke("clear_ai_provider", { provider }));
}

export async function listOllamaModels(baseUrl?: string) {
  if (!isTauri()) return [ollamaModelSchema.parse({ name: "kyra-fake-v1", digest: "fixture-v1", size: 0 })];
  return zodArray(ollamaModelSchema, await invoke("list_ollama_models", { baseUrl }));
}

export async function testAiProvider() {
  if (!isTauri()) {
    fixtureAiStatus = { ...fixtureAiStatus, state: "ready", activatedModel: fixtureAiStatus.requestedModel ?? "kyra-fake-v1" };
    return activationReportSchema.parse({
      fingerprint: "fixture-activation-v1",
      provider: fixtureAiStatus.provider ?? "ollama",
      requestedModel: fixtureAiStatus.requestedModel ?? "kyra-fake-v1",
      resolvedModel: fixtureAiStatus.requestedModel ?? "kyra-fake-v1",
      casesRun: 12,
      schemaValidity: 1,
      evidenceValidity: 1,
      requiredActionCoverage: 1,
      confirmedMeetingRecall: 1,
      unauthorizedActions: 0,
      ambiguousCalendarActions: 0,
      maxLatencyMs: 1,
      passed: true,
    });
  }
  return activationReportSchema.parse(await invoke("test_ai_provider"));
}

export async function runAiNow(): Promise<AiEngineStatus> {
  if (!isTauri()) return aiEngineStatusSchema.parse(fixtureAiStatus);
  return aiEngineStatusSchema.parse(await invoke("run_ai_now"));
}

export async function executeAiCommand(text: string, sessionId?: string): Promise<AiCommandResult> {
  const input = aiCommandInputSchema.parse({ text, sessionId });
  if (!isTauri()) {
    if (/\b(sometime|maybe|not sure)\b/i.test(input.text) && !input.sessionId) {
      return aiCommandResultSchema.parse({ kind: "clarification_required", sessionId: "fixture-session", question: "When should I do that?", expiresAt: new Date(Date.now() + 600_000).toISOString() });
    }
    return aiCommandResultSchema.parse({ kind: "executed", actionIds: ["fixture-action"] });
  }
  return aiCommandResultSchema.parse(await invoke("execute_ai_command", { input }));
}

export async function listAiReviews(): Promise<AiReview[]> {
  if (!isTauri()) return [];
  return zodArray(aiReviewSchema, await invoke("list_ai_reviews"));
}

export async function resolveAiReview(reviewId: string, decision: "accept" | "dismiss") {
  const input = resolveAiReviewInputSchema.parse({ reviewId, decision });
  if (!isTauri()) return [] as string[];
  return await invoke<string[]>("resolve_ai_review", { input });
}

export async function listAiActivity(): Promise<AiActivity[]> {
  if (!isTauri()) return [];
  return zodArray(aiActivitySchema, await invoke("list_ai_activity"));
}

export async function retryAiJob(jobId: string): Promise<AiEngineStatus> {
  if (!isTauri()) return aiEngineStatusSchema.parse(fixtureAiStatus);
  return aiEngineStatusSchema.parse(await invoke("retry_ai_job", { jobId }));
}

export async function revertAiAction(actionId: string) {
  if (!isTauri()) return revertAiActionResultSchema.parse({ actionId, status: "reverted", message: "Fixture action reverted." });
  return revertAiActionResultSchema.parse(await invoke("revert_ai_action", { actionId }));
}

function zodArray<T>(schema: { parse(value: unknown): T }, value: unknown): T[] {
  if (!Array.isArray(value)) throw new Error("Kyra received an invalid native response.");
  return value.map((item) => schema.parse(item));
}
