import { z } from "zod";

export const loopOwnerSchema = z.enum(["me", "them"]);
export const loopStatusSchema = z.enum(["open", "waiting", "done", "dismissed"]);

export const evidenceSchema = z.object({
  id: z.string(),
  sourceKind: z.string(),
  sourceLabel: z.string(),
  excerpt: z.string(),
  occurredAt: z.string(),
});

export const openLoopSchema = z.object({
  id: z.string(),
  title: z.string(),
  summary: z.string(),
  owner: loopOwnerSchema,
  status: loopStatusSchema,
  priority: z.number(),
  dueAt: z.string().nullable(),
  version: z.number(),
  evidence: z.array(evidenceSchema),
  lifecycle: z.enum(["active", "resolved", "dismissed"]).default("active"),
  ownership: z.enum(["me", "other", "shared", "unknown"]).default("me"),
  reviewState: z.enum(["none", "needs_review"]).default("none"),
  scheduled: z.boolean().default(false),
});

export const calendarBlockSchema = z.object({
  id: z.string(),
  title: z.string(),
  startAt: z.string(),
  endAt: z.string(),
  kind: z.enum(["meeting", "execution", "routine"]),
  color: z.string(),
  origin: z.enum(["demo", "local", "google"]).default("demo"),
  externalId: z.string().nullable().optional(),
  etag: z.string().nullable().optional(),
});

export const connectorStateSchema = z.enum([
  "disconnected",
  "connecting",
  "syncing",
  "connected",
  "reconnect_required",
  "error",
]);

export const googleConnectorStatusSchema = z.object({
  state: connectorStateSchema,
  accountEmail: z.string().nullable().optional(),
  lastSyncAt: z.string().nullable().optional(),
  nextSyncAt: z.string().nullable().optional(),
  gmailMessageCount: z.number(),
  calendarEventCount: z.number(),
  lastError: z.string().nullable().optional(),
});

export const googleSyncSummarySchema = z.object({
  gmailMessageCount: z.number(),
  calendarEventCount: z.number(),
  completedAt: z.string(),
});

export const calendarMutationResultSchema = z.object({
  operationId: z.string(),
  event: calendarBlockSchema.nullable().optional(),
  deleted: z.boolean(),
});

export const dashboardSchema = z.object({
  today: z.string(),
  now: z.string(),
  briefing: z.string(),
  openLoops: z.array(openLoopSchema),
  calendarBlocks: z.array(calendarBlockSchema),
});

export const aiProviderSchema = z.enum(["openai", "anthropic", "ollama"]);
export const saveAiProviderConfigSchema = z.object({
  provider: aiProviderSchema,
  model: z.string().trim().min(1).max(160),
  apiKey: z.string().optional(),
  baseUrl: z.string().optional(),
});
export const aiCommandInputSchema = z.object({
  text: z.string().trim().min(1).max(4_000),
  sessionId: z.string().optional(),
});
export const resolveAiReviewInputSchema = z.object({
  reviewId: z.string().min(1),
  decision: z.enum(["accept", "dismiss"]),
});
export const aiEngineStateSchema = z.enum([
  "disconnected",
  "testing",
  "ready",
  "running",
  "paused",
  "blocked",
  "error",
]);
export const aiEngineStatusSchema = z.object({
  state: aiEngineStateSchema,
  provider: aiProviderSchema.nullable().optional(),
  requestedModel: z.string().nullable().optional(),
  activatedModel: z.string().nullable().optional(),
  activationExpiresAt: z.string().nullable().optional(),
  lastRunAt: z.string().nullable().optional(),
  nextRunAt: z.string().nullable().optional(),
  queuedJobs: z.number(),
  runningJobs: z.number(),
  failedJobs: z.number(),
  reviewCount: z.number(),
  lastError: z.string().nullable().optional(),
});

export const activationReportSchema = z.object({
  fingerprint: z.string(),
  provider: aiProviderSchema,
  requestedModel: z.string(),
  resolvedModel: z.string(),
  casesRun: z.number(),
  schemaValidity: z.number(),
  evidenceValidity: z.number(),
  requiredActionCoverage: z.number(),
  confirmedMeetingRecall: z.number(),
  unauthorizedActions: z.number(),
  ambiguousCalendarActions: z.number(),
  maxLatencyMs: z.number(),
  failedCases: z.array(z.string()),
  passed: z.boolean(),
});

export const ollamaModelSchema = z.object({
  name: z.string(),
  digest: z.string(),
  size: z.number(),
});

export const aiReviewSchema = z.object({
  id: z.string(),
  kind: z.string(),
  title: z.string(),
  summary: z.string(),
  evidence: z.array(z.string()),
  irreversibleEffects: z.array(z.string()),
  createdAt: z.string(),
});

export const aiActivitySchema = z.object({
  id: z.string(),
  kind: z.string(),
  status: z.string(),
  title: z.string(),
  detail: z.string(),
  canRevert: z.boolean(),
  compensation: z.boolean(),
  irreversibleEffects: z.array(z.string()),
  createdAt: z.string(),
});

export const aiCommandResultSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("executed"), actionIds: z.array(z.string()) }),
  z.object({
    kind: z.literal("clarification_required"),
    sessionId: z.string(),
    question: z.string(),
    expiresAt: z.string(),
  }),
  z.object({ kind: z.literal("review_created"), reviewId: z.string() }),
  z.object({ kind: z.literal("no_action"), reason: z.string() }),
]);

export const revertAiActionResultSchema = z.object({
  actionId: z.string(),
  status: z.string(),
  message: z.string(),
});

export type Evidence = z.infer<typeof evidenceSchema>;
export type OpenLoop = z.infer<typeof openLoopSchema>;
export type CalendarBlock = z.infer<typeof calendarBlockSchema>;
export type Dashboard = z.infer<typeof dashboardSchema>;
export type LoopStatus = z.infer<typeof loopStatusSchema>;
export type ConnectorState = z.infer<typeof connectorStateSchema>;
export type GoogleConnectorStatus = z.infer<typeof googleConnectorStatusSchema>;
export type GoogleSyncSummary = z.infer<typeof googleSyncSummarySchema>;
export type AiProvider = z.infer<typeof aiProviderSchema>;
export type AiEngineState = z.infer<typeof aiEngineStateSchema>;
export type AiEngineStatus = z.infer<typeof aiEngineStatusSchema>;
export type ActivationReport = z.infer<typeof activationReportSchema>;
export type OllamaModel = z.infer<typeof ollamaModelSchema>;
export type AiReview = z.infer<typeof aiReviewSchema>;
export type AiActivity = z.infer<typeof aiActivitySchema>;
export type AiCommandResult = z.infer<typeof aiCommandResultSchema>;
export type RevertAiActionResult = z.infer<typeof revertAiActionResultSchema>;

export type SaveAiProviderConfigInput = z.infer<typeof saveAiProviderConfigSchema>;

export type CalendarWhen =
  | { kind: "timed"; startAt: string; endAt: string; timeZone: string }
  | { kind: "allDay"; startDate: string; endDate: string };

export interface CalendarEventInput {
  title: string;
  description?: string;
  location?: string;
  when: CalendarWhen;
  attendees?: string[];
  recurrence?: string[];
  sendUpdates?: "all" | "externalOnly" | "none";
}

export type CalendarMutationInput =
  | { action: "create"; operationId: string; event: CalendarEventInput }
  | { action: "update"; operationId: string; eventId: string; expectedEtag: string; patch: Partial<CalendarEventInput> }
  | { action: "delete"; operationId: string; eventId: string; expectedEtag: string; sendUpdates: "all" | "externalOnly" | "none" };
