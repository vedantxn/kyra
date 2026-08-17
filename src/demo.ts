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
  briefing: "Manish and Ayush still owe you the video edits and the write-up, while the 83(b) mailing to Phalanshu and RC's update on the pitch are still on you.",
  openLoops: [
    {
      id: "waiting-manish",
      title: "Waiting on Manish for the video edits",
      summary: "You followed up asking why you haven't gotten any edited videos, and Manish said his editor just started and will send a few by morning.",
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
      summary: "You pushed Ayush hard for the write-up and asked him to send it by tomorrow; he said he'd send it and add more new stuff, but it hasn't arrived yet.",
      owner: "them",
      status: "waiting",
      priority: 90,
      dueAt: at(10),
      version: 1,
      evidence: [{ id: "e2", sourceKind: "fixture_message", sourceLabel: "Message with Ayush", excerpt: "I'll send it tomorrow and add the new material.", occurredAt: at(21, 15, -1) }],
    },
    {
      id: "mail-receipt",
      title: "Print, sign, mail the 83(b) form via USPS and send Phalanshu the receipt",
      summary: "You told Phalanshu you'd do the 83(b) mailing, and he reminded you to keep the USPS receipt as proof — this is still pending on your end.",
      owner: "me",
      status: "open",
      priority: 86,
      dueAt: at(17),
      version: 1,
      evidence: [{ id: "e3", sourceKind: "gmail", sourceLabel: "Email from Phalanshu", excerpt: "Please keep the USPS receipt as proof.", occurredAt: at(19, 40, -1) }],
    },
    {
      id: "samarth-sign-doc",
      title: "Samarth to sign the doc tonight",
      summary: "You asked Samarth to sign and he said he'd do it tonight, so you're waiting on him.",
      owner: "them",
      status: "waiting",
      priority: 82,
      dueAt: at(23),
      version: 1,
      evidence: [{ id: "e5", sourceKind: "fixture_message", sourceLabel: "Message with Samarth", excerpt: "I'll sign it tonight.", occurredAt: at(20, 5, -1) }],
    },
    {
      id: "update-rc",
      title: "Update RC on how the pitch/meeting went",
      summary: "RC asked how it went and you only said it's in 20 mins — you still haven't told him the outcome.",
      owner: "me",
      status: "open",
      priority: 78,
      dueAt: null,
      version: 1,
      evidence: [{ id: "e4", sourceKind: "fixture_message", sourceLabel: "Message with RC", excerpt: "How did the pitch go?", occurredAt: at(17, 30, -1) }],
    },
  ],
  calendarBlocks: [
    { id: "sleep", title: "Night time", startAt: at(1), endAt: at(8), kind: "routine", color: "#b7b9b2", origin: "demo" },
    { id: "gym", title: "Gym", startAt: at(8, 30), endAt: at(9, 30), kind: "execution", color: "#8ca481", origin: "demo" },
    { id: "meeting", title: "Kyra product review", startAt: at(10), endAt: at(11), kind: "meeting", color: "#b7b9b2", origin: "demo" },
    { id: "deep-work", title: "Build V1 vertical slice", startAt: at(13), endAt: at(16), kind: "execution", color: "#8ca481", origin: "demo" },
  ],
};
