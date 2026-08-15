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
