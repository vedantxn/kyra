import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { CalendarDays, Check, ChevronDown, Circle, ListTodo, Search, Sparkles, X } from "lucide-react";
import { createCalendarBlock, createTask, getDashboard, hideOverlay, isTauri, setLoopStatus } from "./api";
import { parseCommand } from "./command";
import type { CalendarBlock, Dashboard, OpenLoop } from "./contracts";

const formatDay = (iso: string) => {
  const date = new Date(iso);
  const weekday = new Intl.DateTimeFormat("en", { weekday: "short" }).format(date);
  const day = new Intl.DateTimeFormat("en", { day: "numeric" }).format(date);
  const month = new Intl.DateTimeFormat("en", { month: "short" }).format(date);
  return `${weekday} ${day} ${month}`;
};

const formatTime = (iso: string) =>
  new Intl.DateTimeFormat("en", { hour: "numeric", minute: "2-digit" }).format(new Date(iso));

const hourPosition = (iso: string) => {
  const date = new Date(iso);
  return ((date.getHours() * 60 + date.getMinutes()) / 1440) * 100;
};

function Logo() {
  return <span className="logo" aria-hidden="true"><i /></span>;
}

function Timeline({ blocks, now, onExpand }: { blocks: CalendarBlock[]; now: string; onExpand: () => void }) {
  const hours = [0, 3, 6, 9, 12, 15, 18, 21, 24];
  return (
    <section className="timeline-panel" aria-label="Today's calendar">
      <button className="date-button" onClick={onExpand}>
        {formatDay(now)} <ChevronDown size={14} />
      </button>
      <div className="timeline">
        {hours.map((hour) => (
          <div className="hour" key={hour} style={{ top: `${(hour / 24) * 100}%` }}>
            <span>{hour === 24 ? "12 AM" : new Intl.DateTimeFormat("en", { hour: "numeric" }).format(new Date(2026, 0, 1, hour))}</span>
            <b />
          </div>
        ))}
        <div className="timeline-track" />
        {blocks.map((block) => {
          const top = hourPosition(block.startAt);
          const height = Math.max(1.25, hourPosition(block.endAt) - top);
          return (
            <div
              className={`timeline-block ${block.kind}`}
              key={block.id}
              style={{ top: `${top}%`, height: `${height}%` }}
              title={`${block.title}, ${formatTime(block.startAt)} to ${formatTime(block.endAt)}`}
            >
              <span>{formatTime(block.startAt)}</span>
              <strong>{block.title}</strong>
            </div>
          );
        })}
        <div className="now-marker" style={{ top: `${hourPosition(now)}%` }}>
          <i /><span>{formatTime(now)}</span>
        </div>
      </div>
      <button className="planner-trigger" onClick={onExpand} aria-label="Open the next three days">
        <CalendarDays size={14} /><span>3 days</span>
      </button>
    </section>
  );
}

function Loops({ loops, onComplete }: { loops: OpenLoop[]; onComplete: (loop: OpenLoop) => void }) {
  const items = loops
    .filter((loop) => loop.status !== "done" && loop.status !== "dismissed")
    .sort((a, b) => b.priority - a.priority);

  return (
    <aside className="loops-panel" aria-label="Open loops">
      <p className="loops-heading">TO BE DONE</p>
      <div className="loop-list">
        {items.map((loop) => (
          <article className="loop-card" key={loop.id}>
            <button className="loop-check" aria-label={`Mark ${loop.title} done`} onClick={() => onComplete(loop)}>
              <Circle size={20} />
            </button>
            <div>
              <h3>{loop.title}</h3>
              <p>{loop.summary}</p>
            </div>
          </article>
        ))}
      </div>
      {items.length === 0 && <div className="empty-state"><Check size={19} /> Nothing is slipping through.</div>}
    </aside>
  );
}

function CommandPalette({
  onTask,
  onCalendar,
}: {
  onTask: (title: string) => Promise<void>;
  onCalendar: (title: string, startAt: string, endAt: string) => Promise<void>;
}) {
  const [value, setValue] = useState("");
  const [message, setMessage] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = (event: KeyboardEvent) => {
      if (event.metaKey && event.key.toLowerCase() === "k") {
        event.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", focus);
    return () => window.removeEventListener("keydown", focus);
  }, []);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseCommand(value);
    if (parsed.kind === "unknown") {
      setMessage(parsed.message);
      return;
    }
    if (parsed.kind === "task") await onTask(parsed.title);
    if (parsed.kind === "calendar") await onCalendar(parsed.title, parsed.startAt, parsed.endAt);
    setMessage(parsed.kind === "task" ? `Added: ${parsed.title}` : `Protected: ${parsed.title}`);
    setValue("");
  };

  return (
    <>
      <form className="command-palette" onSubmit={submit}>
        <div className="command-input">
          <Search size={22} />
          <input
            ref={inputRef}
            value={value}
            onChange={(event) => { setValue(event.target.value); setMessage(""); }}
            placeholder="What do you wanna get done?"
            aria-label="Kyra command"
          />
          <kbd>ESC</kbd>
        </div>
        <button type="button" onClick={() => { setValue("/cal "); inputRef.current?.focus(); }} className={value === "" || value.startsWith("/cal") ? "active" : ""}>
          <CalendarDays size={16} /><strong>/cal</strong><span>what and when — e.g. standup tomorrow 9am</span><b>↵</b>
        </button>
        <button type="button" onClick={() => { setValue("/task "); inputRef.current?.focus(); }} className={value.startsWith("/task") ? "active" : ""}>
          <ListTodo size={16} /><strong>/task</strong><span>what needs doing</span><b>↵</b>
        </button>
      </form>
      {message && <output className="capture-message">{message}</output>}
    </>
  );
}

type PlannerBlock = CalendarBlock & { plannerDay: number };

function plannerDate(dayOffset: number, hours: number, minutes = 0) {
  const date = new Date();
  date.setDate(date.getDate() + dayOffset);
  date.setHours(hours, minutes, 0, 0);
  return date.toISOString();
}

function Planner({ blocks, onClose }: { blocks: CalendarBlock[]; onClose: () => void }) {
  const days = Array.from({ length: 3 }, (_, index) => {
    const date = new Date();
    date.setDate(date.getDate() + index);
    return date;
  });
  const hours = Array.from({ length: 16 }, (_, index) => index + 1);
  const recurring: PlannerBlock[] = days.flatMap((_, plannerDay) => [
    { id: `sleep-${plannerDay}`, title: "Night time", startAt: plannerDate(plannerDay, 1), endAt: plannerDate(plannerDay, 8), kind: "routine", color: "#b7b9b2", plannerDay },
    { id: `gym-${plannerDay}`, title: "gym", startAt: plannerDate(plannerDay, 8, plannerDay === 0 ? 45 : 30), endAt: plannerDate(plannerDay, 9, plannerDay === 0 ? 45 : 30), kind: "execution", color: "#8ca481", plannerDay },
  ]);
  const supplied: PlannerBlock[] = blocks
    .filter((block) => !/night time|gym/i.test(block.title))
    .map((block) => {
      const blockDate = new Date(block.startAt);
      const plannerDay = Math.max(0, Math.min(2, Math.round((blockDate.setHours(0, 0, 0, 0) - new Date().setHours(0, 0, 0, 0)) / 86_400_000)));
      return { ...block, plannerDay };
    });
  const samples: PlannerBlock[] = [
    { id: "review-reminder", title: "reminder: meeting w Rajeev", startAt: plannerDate(0, 12), endAt: plannerDate(0, 12, 30), kind: "meeting", color: "#b7b9b2", plannerDay: 0 },
    { id: "wordware", title: "Maybe: Wordware office visit", startAt: plannerDate(1, 14, 30), endAt: plannerDate(1, 15, 30), kind: "meeting", color: "#b7b9b2", plannerDay: 1 },
    { id: "aditya", title: "Sahil (Kyra) / Aditya", startAt: plannerDate(2, 10), endAt: plannerDate(2, 10, 30), kind: "meeting", color: "#b7b9b2", plannerDay: 2 },
  ];
  const plannerBlocks = [...recurring, ...supplied, ...samples];

  return (
    <div className="planner-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="planner" aria-label="Next three days">
        <header>
          <h2>Next three days</h2>
          <div className="legend"><span><i className="meeting" /> meeting</span><span><i className="execution" /> execution</span></div>
          <button onClick={onClose} aria-label="Close calendar"><X size={16} /></button>
        </header>
        <div className="planner-grid">
          <div className="planner-hours">{hours.map((hour) => <span key={hour} style={{ top: `${((hour - 1) / 16) * 100}%` }}>{new Intl.DateTimeFormat("en", { hour: "numeric" }).format(new Date(2026, 0, 1, hour))}</span>)}</div>
          {days.map((day, dayIndex) => (
            <div className="planner-day" key={day.toISOString()}>
              <h3>{formatDay(day.toISOString())}<small>{dayIndex === 0 ? "TODAY" : dayIndex === 1 ? "TOMORROW" : ""}</small></h3>
              <div className="planner-lines">{hours.map((hour) => <i key={hour} />)}</div>
              <div className="planner-events">
                {plannerBlocks.filter((block) => block.plannerDay === dayIndex).map((block) => {
                  const start = new Date(block.startAt);
                  const end = new Date(block.endAt);
                  const startMinutes = start.getHours() * 60 + start.getMinutes() - 60;
                  const duration = (end.getTime() - start.getTime()) / 60_000;
                  return (
                    <article className={`planner-event ${block.kind}`} key={block.id} style={{ top: `${(startMinutes / 960) * 100}%`, height: `${Math.max(4.4, (duration / 960) * 100)}%` }}>
                      <strong>{block.title}</strong><span>{formatTime(block.startAt)} – {formatTime(block.endAt)}</span>
                    </article>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
        <footer>drag a block to move it · its edges to resize · empty grid for a new one</footer>
      </section>
    </div>
  );
}

export default function App() {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [plannerOpen, setPlannerOpen] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    getDashboard().then(setDashboard).catch((cause) => setError(String(cause)));
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (plannerOpen) setPlannerOpen(false);
      else void hideOverlay();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [plannerOpen]);

  const visibleLoops = useMemo(
    () => dashboard?.openLoops.filter((loop) => loop.status !== "done" && loop.status !== "dismissed") ?? [],
    [dashboard],
  );

  const addTask = async (title: string) => {
    const loop = await createTask(title);
    if (isTauri()) setDashboard(await getDashboard());
    else setDashboard((current) => current ? { ...current, openLoops: [...current.openLoops, loop] } : current);
  };

  const addCalendar = async (title: string, startAt: string, endAt: string) => {
    const block = await createCalendarBlock(title, startAt, endAt);
    if (isTauri()) setDashboard(await getDashboard());
    else setDashboard((current) => current ? { ...current, calendarBlocks: [...current.calendarBlocks, block] } : current);
  };

  const complete = async (loop: OpenLoop) => {
    try {
      const updated = await setLoopStatus(loop.id, "done", loop.version);
      if (isTauri()) setDashboard(await getDashboard());
      else setDashboard((current) => current ? { ...current, openLoops: current.openLoops.map((item) => item.id === updated.id ? updated : item) } : current);
    } catch (cause) {
      setError(String(cause));
    }
  };

  if (error) return <main className="fatal"><Sparkles /><h1>Kyra could not start</h1><p>{error}</p></main>;
  if (!dashboard) return <main className="loading"><Logo /><span>Connecting the day…</span></main>;

  return (
    <main className="app-shell">
      <Timeline blocks={dashboard.calendarBlocks} now={dashboard.now} onExpand={() => setPlannerOpen(true)} />
      <section className="night-panel">
        <div className="night-content">
          <div className="night-title"><Logo /><span>Night</span></div>
          <p>{dashboard.briefing}</p>
        </div>
        <CommandPalette onTask={addTask} onCalendar={addCalendar} />
      </section>
      <Loops loops={dashboard.openLoops} onComplete={complete} />
      <span className="loop-count">{37 + Math.max(0, visibleLoops.length - 5)}</span>
      <div className="status-dot" title="Kyra is connected" />
      {plannerOpen && <Planner blocks={dashboard.calendarBlocks} onClose={() => setPlannerOpen(false)} />}
    </main>
  );
}
