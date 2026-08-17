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

export type Evidence = z.infer<typeof evidenceSchema>;
export type OpenLoop = z.infer<typeof openLoopSchema>;
export type CalendarBlock = z.infer<typeof calendarBlockSchema>;
export type Dashboard = z.infer<typeof dashboardSchema>;
export type LoopStatus = z.infer<typeof loopStatusSchema>;
export type ConnectorState = z.infer<typeof connectorStateSchema>;
export type GoogleConnectorStatus = z.infer<typeof googleConnectorStatusSchema>;
export type GoogleSyncSummary = z.infer<typeof googleSyncSummarySchema>;

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
