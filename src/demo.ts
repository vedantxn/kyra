import type { Dashboard } from "./contracts";

const now = new Date();
const at = (hours: number, minutes = 0, dayOffset = 0) => {
  const date = new Date(now);
  date.setDate(date.getDate() + dayOffset);
  date.setHours(hours, minutes, 0, 0);
  return date.toISOString();
};
export const demoDashboard: Dashboard = {
  today: now.toISOString(),
  now: now.toISOString(),
  briefing: "Manish and Ayush still owe you the video edits and write-up. The mailing receipt and RC's update are still on you.",
  openLoops: [
    {
      id: "waiting-manish",
      title: "Waiting on Manish for the video edits",
      summary: "You followed up, and Manish said his editor started and would send a few by morning.",
      owner: "them",
      status: "waiting",
      priority: 95,
      dueAt: at(9),
      version: 1,
      evidence: [{ id: "e1", sourceKind: "fixture_message", sourceLabel: "Message with Manish", excerpt: "My editor just started. Will send a few by morning.", occurredAt: at(23, 20, -1) }],
    },
    {
      id: "waiting-ayush",
      title: "Waiting on Ayush for the write-up",
      summary: "Ayush promised the write-up for today, but it has not arrived yet.",
      owner: "them",
      status: "waiting",
      priority: 90,
      dueAt: at(10),
      version: 1,
      evidence: [{ id: "e2", sourceKind: "fixture_message", sourceLabel: "Message with Ayush", excerpt: "I'll send it tomorrow and add the new material.", occurredAt: at(21, 15, -1) }],
    },
    {
      id: "mail-receipt",
      title: "Mail the signed form and send the receipt",
      summary: "You said you would mail the form and keep the receipt as proof.",
      owner: "me",
      status: "open",
      priority: 86,
      dueAt: at(17),
      version: 1,
      evidence: [{ id: "e3", sourceKind: "gmail", sourceLabel: "Email from Phalanshu", excerpt: "Please keep the USPS receipt as proof.", occurredAt: at(19, 40, -1) }],
    },
    {
      id: "update-rc",
      title: "Update RC on how the pitch went",
      summary: "RC asked how it went; you still have not shared the outcome.",
      owner: "me",
      status: "open",
      priority: 78,
      dueAt: null,
      version: 1,
      evidence: [{ id: "e4", sourceKind: "fixture_message", sourceLabel: "Message with RC", excerpt: "How did the pitch go?", occurredAt: at(17, 30, -1) }],
    },
  ],
  calendarBlocks: [
    { id: "sleep", title: "Night time", startAt: at(1), endAt: at(8), kind: "routine", color: "#a8c7ee" },
    { id: "gym", title: "Gym", startAt: at(8, 30), endAt: at(9, 30), kind: "execution", color: "#7bcaa2" },
    { id: "meeting", title: "Kyra product review", startAt: at(10), endAt: at(11), kind: "meeting", color: "#8eb8ec" },
    { id: "deep-work", title: "Build V1 vertical slice", startAt: at(13), endAt: at(16), kind: "execution", color: "#86c998" },
  ],
};
