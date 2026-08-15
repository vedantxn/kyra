import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownRight,
  CalendarDays,
  Check,
  ChevronDown,
  Circle,
  Command,
  ListTodo,
  Maximize2,
  Search,
  Sparkles,
  X,
} from "lucide-react";
import { createCalendarBlock, createTask, getDashboard, hideOverlay, isTauri, setLoopStatus } from "./api";
import { parseCommand } from "./command";
import type { CalendarBlock, Dashboard, OpenLoop } from "./contracts";

const formatDay = (iso: string) =>
  new Intl.DateTimeFormat("en", { weekday: "short", day: "numeric", month: "short" }).format(new Date(iso));

const formatTime = (iso: string) =>
  new Intl.DateTimeFormat("en", { hour: "numeric", minute: "2-digit" }).format(new Date(iso));

const hourPosition = (iso: string) => {
  const date = new Date(iso);
  return ((date.getHours() * 60 + date.getMinutes()) / 1440) * 100;
};

function Logo() {
  return (
    <span className="logo" aria-hidden="true">
      {Array.from({ length: 8 }).map((_, index) => (
        <i key={index} style={{ transform: `rotate(${index * 45}deg) translateY(-7px)` }} />
      ))}
    </span>
  );
}

function Timeline({ blocks, now, onExpand }: { blocks: CalendarBlock[]; now: string; onExpand: () => void }) {
  const hours = [0, 3, 6, 9, 12, 15, 18, 21, 24];
  return (
    <section className="timeline-panel" aria-label="Today's calendar">
      <button className="date-button" onClick={onExpand}>
        {formatDay(now)} <ChevronDown size={15} />
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
          const height = Math.max(1.5, hourPosition(block.endAt) - top);
          return (
            <div
              className={`timeline-block ${block.kind}`}
              key={block.id}
              style={{ top: `${top}%`, height: `${height}%`, borderColor: block.color }}
              title={`${block.title}, ${formatTime(block.startAt)} to ${formatTime(block.endAt)}`}
            >
              <span>{formatTime(block.startAt)}</span>
              <strong>{block.title}</strong>
            </div>
          );
        })}
        <div className="now-marker" style={{ top: `${hourPosition(now)}%` }}>
          <i /> <span>{formatTime(now)}</span>
        </div>
      </div>
      <button className="expand-calendar" onClick={onExpand}>
        <CalendarDays size={16} /> Next three days <Maximize2 size={14} />
      </button>
    </section>
  );
}

function Loops({ loops, onComplete }: { loops: OpenLoop[]; onComplete: (loop: OpenLoop) => void }) {
  const onMe = loops.filter((loop) => loop.owner === "me" && loop.status !== "done");
  const waiting = loops.filter((loop) => loop.owner === "them" && loop.status !== "done");

  const group = (label: string, items: OpenLoop[]) => (
    <div className="loop-group">
      <p className="eyebrow">{label}</p>
      {items.map((loop) => (
        <article className="loop-card" key={loop.id}>
          <button className="loop-check" aria-label={`Mark ${loop.title} done`} onClick={() => onComplete(loop)}>
            <Circle size={21} />
          </button>
          <div>
            <h3>{loop.title}</h3>
            <p>{loop.summary}</p>
            {loop.evidence[0] && (
              <button className="evidence-chip" title={loop.evidence[0].excerpt}>
                <ArrowDownRight size={13} /> {loop.evidence[0].sourceLabel}
              </button>
            )}
          </div>
        </article>
      ))}
    </div>
  );

  return (
    <aside className="loops-panel" aria-label="Open loops">
      {group("ON ME", onMe)}
      {group("WAITING ON", waiting)}
      {onMe.length + waiting.length === 0 && (
        <div className="empty-state"><Check size={22} /> Nothing is slipping through.</div>
      )}
    </aside>
  );
}

function CommandPalette({ onTask, onCalendar }: { onTask: (title: string) => Promise<void>; onCalendar: (title: string, startAt: string, endAt: string) => Promise<void> }) {
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
    setMessage(parsed.kind === "task" ? "Task captured" : "Time protected");
    setValue("");
  };

  return (
    <form className="command-palette" onSubmit={submit}>
      <div className="command-input">
        <Search size={24} />
        <input
          ref={inputRef}
          value={value}
          onChange={(event) => { setValue(event.target.value); setMessage(""); }}
          placeholder="What do you want to get done?"
          aria-label="Kyra command"
        />
        <kbd>ESC</kbd>
      </div>
      <button type="button" onClick={() => setValue("/cal ")} className={value.startsWith("/cal") ? "active" : ""}>
        <CalendarDays size={17} /> <strong>/cal</strong><span>what and when — e.g. standup 9am</span><b>↵</b>
      </button>
      <button type="button" onClick={() => setValue("/task ")} className={value.startsWith("/task") ? "active" : ""}>
        <ListTodo size={17} /> <strong>/task</strong><span>what needs doing</span><b>↵</b>
      </button>
      {message && <output>{message}</output>}
    </form>
  );
}

function Planner({ blocks, onClose }: { blocks: CalendarBlock[]; onClose: () => void }) {
  const days = Array.from({ length: 3 }, (_, index) => {
    const date = new Date();
    date.setDate(date.getDate() + index);
    return date;
  });
  const hours = Array.from({ length: 17 }, (_, index) => index + 1);
  return (
    <div className="planner-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="planner" aria-label="Next three days">
        <header>
          <div><p className="eyebrow">CALENDAR</p><h2>Next three days</h2></div>
          <div className="legend"><span><i className="meeting" /> meeting</span><span><i className="execution" /> execution</span></div>
          <button onClick={onClose} aria-label="Close calendar"><X size={18} /></button>
        </header>
        <div className="planner-grid">
          <div className="planner-hours">{hours.map((hour) => <span key={hour} style={{ top: `${((hour - 1) / 17) * 100}%` }}>{new Intl.DateTimeFormat("en", { hour: "numeric" }).format(new Date(2026, 0, 1, hour))}</span>)}</div>
          {days.map((day, dayIndex) => (
            <div className="planner-day" key={day.toISOString()}>
              <h3>{formatDay(day.toISOString())}<small>{dayIndex === 0 ? "TODAY" : dayIndex === 1 ? "TOMORROW" : ""}</small></h3>
              <div className="planner-lines">{hours.map((hour) => <i key={hour} />)}</div>
              <div className="planner-events">
                {blocks.filter((block) => new Date(block.startAt).getDate() === day.getDate()).map((block) => {
                  const start = new Date(block.startAt);
                  const end = new Date(block.endAt);
                  const startMinutes = start.getHours() * 60 + start.getMinutes() - 60;
                  const duration = (end.getTime() - start.getTime()) / 60_000;
                  return <article className={`planner-event ${block.kind}`} key={block.id} style={{ top: `${(startMinutes / 1020) * 100}%`, height: `${Math.max(5, (duration / 1020) * 100)}%` }}><strong>{block.title}</strong><span>{formatTime(block.startAt)} – {formatTime(block.endAt)}</span></article>;
                })}
              </div>
            </div>
          ))}
        </div>
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

  const visibleLoops = useMemo(() => dashboard?.openLoops.filter((loop) => loop.status !== "done" && loop.status !== "dismissed") ?? [], [dashboard]);

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
      <div className="ambient ambient-one" /><div className="ambient ambient-two" /><div className="grain" />
      <Timeline blocks={dashboard.calendarBlocks} now={dashboard.now} onExpand={() => setPlannerOpen(true)} />
      <section className="night-panel">
        <div className="night-content">
          <div className="night-title"><Logo /><span>Night</span></div>
          <p>{dashboard.briefing}</p>
          <div className="focus-count"><Sparkles size={14} /> {visibleLoops.length} open loops in focus</div>
        </div>
        <CommandPalette onTask={addTask} onCalendar={addCalendar} />
      </section>
      <Loops loops={dashboard.openLoops} onComplete={complete} />
      <div className="status-dot" title="Kyra is connected" />
      <div className="shortcut-hint"><Command size={13} /> K</div>
      {plannerOpen && <Planner blocks={dashboard.calendarBlocks} onClose={() => setPlannerOpen(false)} />}
    </main>
  );
}
