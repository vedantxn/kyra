export type ParsedCommand =
  | { kind: "task"; title: string }
  | { kind: "calendar"; title: string; startAt: string; endAt: string }
  | { kind: "unknown"; message: string };

export function parseCommand(value: string, now = new Date()): ParsedCommand {
  const trimmed = value.trim();
  const task = trimmed.match(/^\/task\s+(.+)$/i);
  if (task) return { kind: "task", title: task[1].trim() };

  const calendar = trimmed.match(/^\/cal\s+(.+?)\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/i);
  if (calendar) {
    let hour = Number(calendar[2]);
    const minute = Number(calendar[3] ?? 0);
    const meridiem = calendar[4]?.toLowerCase();
    if (meridiem === "pm" && hour < 12) hour += 12;
    if (meridiem === "am" && hour === 12) hour = 0;
    if (hour > 23 || minute > 59) return { kind: "unknown", message: "That time is not valid." };

    const start = new Date(now);
    start.setHours(hour, minute, 0, 0);
    const end = new Date(start.getTime() + 60 * 60 * 1000);
    return { kind: "calendar", title: calendar[1].trim(), startAt: start.toISOString(), endAt: end.toISOString() };
  }

  return { kind: "unknown", message: "Start with /task or /cal." };
}
