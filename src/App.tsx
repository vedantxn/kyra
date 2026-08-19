import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity, AlertCircle, ArrowRight, Bot, CalendarDays, Check, CheckCircle2, ChevronDown, Circle, Cloud, KeyRound, ListTodo, LoaderCircle, Mail, RefreshCw, RotateCcw, Search, Server, Settings, ShieldCheck, Sparkles, Unplug, X } from "lucide-react";
import {
  clearAiProvider,
  connectGoogle,
  createCalendarBlock,
  createTask,
  disconnectGoogle,
  executeAiCommand,
  getAiEngineStatus,
  getDashboard,
  getGoogleConnectorStatus,
  hideOverlay,
  isTauri,
  listAiActivity,
  listAiReviews,
  listOllamaModels,
  mutateGoogleCalendar,
  resolveAiReview,
  retryAiJob,
  revertAiAction,
  runAiNow,
  saveAiProviderConfig,
  setLoopStatus,
  syncGoogleNow,
  testAiProvider,
} from "./api";
import { parseCommand } from "./command";
import type { AiActivity, AiCommandResult, AiEngineStatus, AiProvider, AiReview, CalendarBlock, Dashboard, GoogleConnectorStatus, OpenLoop, SaveAiProviderConfigInput } from "./contracts";
import { readSetupPreference, shouldShowSetup, writeSetupPreference } from "./setup";

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
  const currentDay = new Date(now).toDateString();
  const todayBlocks = blocks.filter((block) => new Date(block.startAt).toDateString() === currentDay);
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
        {todayBlocks.map((block) => {
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
              {(loop.ownership === "other" || loop.reviewState === "needs_review" || loop.scheduled) && (
                <div className="loop-meta">
                  {loop.ownership === "other" && <span>Waiting</span>}
                  {loop.reviewState === "needs_review" && <span>Needs review</span>}
                  {loop.scheduled && <span>Scheduled</span>}
                </div>
              )}
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
  onNatural,
}: {
  onTask: (title: string) => Promise<void>;
  onCalendar: (title: string, startAt: string, endAt: string) => Promise<void>;
  onNatural: (text: string, sessionId?: string) => Promise<AiCommandResult>;
}) {
  const [value, setValue] = useState("");
  const [message, setMessage] = useState("");
  const [sessionId, setSessionId] = useState<string>();
  const [submitting, setSubmitting] = useState(false);
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
    if (!value.trim() || submitting) return;
    const parsed = parseCommand(value);
    setSubmitting(true);
    try {
      if (parsed.kind === "task") {
        await onTask(parsed.title);
        setSessionId(undefined);
        setMessage(`Added: ${parsed.title}`);
        setValue("");
      } else if (parsed.kind === "calendar") {
        await onCalendar(parsed.title, parsed.startAt, parsed.endAt);
        setSessionId(undefined);
        setMessage(`Added to Google Calendar: ${parsed.title}`);
        setValue("");
      } else {
        const result = await onNatural(value, sessionId);
        if (result.kind === "clarification_required") {
          setSessionId(result.sessionId);
          setMessage(result.question);
          setValue("");
          inputRef.current?.focus();
        } else {
          setSessionId(undefined);
          setValue("");
          setMessage(result.kind === "executed" ? "Done." : result.kind === "review_created" ? "Added to review." : result.reason);
        }
      }
    } catch (cause) {
      setMessage(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSubmitting(false);
    }
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
            placeholder={sessionId ? "Answer Kyra’s question…" : "What do you wanna get done?"}
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

function Planner({ blocks, showDemo, onClose }: { blocks: CalendarBlock[]; showDemo: boolean; onClose: () => void }) {
  const days = Array.from({ length: 3 }, (_, index) => {
    const date = new Date();
    date.setDate(date.getDate() + index);
    return date;
  });
  const hours = Array.from({ length: 16 }, (_, index) => index + 1);
  const recurring: PlannerBlock[] = days.flatMap((_, plannerDay) => [
    { id: `sleep-${plannerDay}`, title: "Night time", startAt: plannerDate(plannerDay, 1), endAt: plannerDate(plannerDay, 8), kind: "routine", color: "#b7b9b2", origin: "demo", plannerDay },
    { id: `gym-${plannerDay}`, title: "gym", startAt: plannerDate(plannerDay, 8, plannerDay === 0 ? 45 : 30), endAt: plannerDate(plannerDay, 9, plannerDay === 0 ? 45 : 30), kind: "execution", color: "#8ca481", origin: "demo", plannerDay },
  ]);
  const supplied: PlannerBlock[] = blocks
    .filter((block) => !/night time|gym/i.test(block.title))
    .map((block) => {
      const blockDate = new Date(block.startAt);
      const plannerDay = Math.round((blockDate.setHours(0, 0, 0, 0) - new Date().setHours(0, 0, 0, 0)) / 86_400_000);
      return { ...block, plannerDay };
    })
    .filter((block) => block.plannerDay >= 0 && block.plannerDay <= 2);
  const samples: PlannerBlock[] = [
    { id: "review-reminder", title: "reminder: meeting w Rajeev", startAt: plannerDate(0, 12), endAt: plannerDate(0, 12, 30), kind: "meeting", color: "#b7b9b2", origin: "demo", plannerDay: 0 },
    { id: "wordware", title: "Maybe: Wordware office visit", startAt: plannerDate(1, 14, 30), endAt: plannerDate(1, 15, 30), kind: "meeting", color: "#b7b9b2", origin: "demo", plannerDay: 1 },
    { id: "aditya", title: "Sahil (Kyra) / Aditya", startAt: plannerDate(2, 10), endAt: plannerDate(2, 10, 30), kind: "meeting", color: "#b7b9b2", origin: "demo", plannerDay: 2 },
  ];
  const plannerBlocks = [...(showDemo ? recurring : []), ...supplied, ...(showDemo ? samples : [])];

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

const connectorLabels: Record<GoogleConnectorStatus["state"], string> = {
  disconnected: "Not connected",
  connecting: "Waiting for Google",
  syncing: "Synchronizing",
  connected: "Connected",
  reconnect_required: "Reconnect required",
  error: "Retry scheduled",
};

const aiLabels: Record<AiEngineStatus["state"], string> = {
  disconnected: "Not activated",
  testing: "Testing model",
  ready: "Ready",
  running: "Running",
  paused: "Paused",
  blocked: "Blocked",
  error: "Needs attention",
};

function formatSyncTime(value?: string | null) {
  if (!value) return "Never";
  return new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export function SetupFlow({
  status,
  busy,
  error,
  onConnect,
  onFinish,
  onExplore,
  onClose,
}: {
  status: GoogleConnectorStatus;
  busy: boolean;
  error: string;
  onConnect: () => Promise<void>;
  onFinish: () => void;
  onExplore: () => void;
  onClose: () => void;
}) {
  const [started, setStarted] = useState(status.state === "connecting" || status.state === "syncing");
  const ready = status.state === "connected" && Boolean(status.lastSyncAt);
  const working = busy || status.state === "connecting" || status.state === "syncing";
  const failed = started && !working && !ready && Boolean(error || status.lastError);

  const begin = async () => {
    setStarted(true);
    await onConnect();
  };

  return (
    <div className="setup-backdrop">
      <section className="setup-dialog" aria-label="Set up Kyra">
        <button className="setup-close" onClick={onClose} disabled={working} aria-label="Close setup"><X size={17} /></button>
        {ready ? (
          <div className="setup-ready">
            <span className="setup-success"><CheckCircle2 size={26} /></span>
            <p className="setup-kicker">WORKSPACE CONNECTED</p>
            <h1>Your real day is ready.</h1>
            <p className="setup-lede">Kyra has finished the first import for <strong>{status.accountEmail}</strong>. Demo events are now out of the way.</p>
            <dl className="setup-counts">
              <div><dt>Gmail</dt><dd>{status.gmailMessageCount}<span>messages indexed</span></dd></div>
              <div><dt>Calendar</dt><dd>{status.calendarEventCount}<span>events synchronized</span></dd></div>
            </dl>
            <p className="setup-last-sync">Last synchronized {formatSyncTime(status.lastSyncAt)}</p>
            <button className="setup-primary" onClick={onFinish}>Open Kyra <ArrowRight size={15} /></button>
            <p className="setup-footnote">Kyra refreshes Google every five minutes while the app is open. Intelligence can be configured later in Settings.</p>
          </div>
        ) : working ? (
          <div className="setup-working" aria-live="polite">
            <span className="setup-loader"><LoaderCircle size={28} /></span>
            <p className="setup-kicker">{status.state === "syncing" ? "IMPORTING YOUR WORKSPACE" : "WAITING FOR GOOGLE"}</p>
            <h1>{status.state === "syncing" ? "Bringing your day into focus." : "Finish connecting in your browser."}</h1>
            <p className="setup-lede">{status.state === "syncing" ? "Kyra is securely importing Gmail and your primary Calendar. Keep this window open for the first sync." : "Google opened in your system browser. Choose your test account and approve the requested access, then return here."}</p>
            <div className="setup-progress">
              <div className={status.accountEmail ? "complete" : "active"}>{status.accountEmail ? <Check size={15} /> : <LoaderCircle size={15} />}<span><strong>Google account</strong><small>{status.accountEmail ?? "Authorization in progress"}</small></span></div>
              <div className={status.state === "syncing" ? "active" : "pending"}>{status.state === "syncing" ? <LoaderCircle size={15} /> : <Circle size={15} />}<span><strong>Gmail</strong><small>Inbox + Sent, last 30 days, up to 500 messages</small></span></div>
              <div className={status.state === "syncing" ? "active" : "pending"}>{status.state === "syncing" ? <LoaderCircle size={15} /> : <Circle size={15} />}<span><strong>Primary Calendar</strong><small>30 days back through 90 days ahead</small></span></div>
            </div>
            <p className="setup-footnote">Nothing is sent or deleted from Gmail. Closing this screen is disabled while the secure connection is in progress.</p>
          </div>
        ) : failed ? (
          <div className="setup-error-state">
            <span className="setup-error-icon"><AlertCircle size={24} /></span>
            <p className="setup-kicker">CONNECTION NEEDS ATTENTION</p>
            <h1>Google did not connect.</h1>
            <p className="setup-lede">{error || status.lastError}</p>
            <button className="setup-primary" onClick={() => void begin()}>Try again <RefreshCw size={14} /></button>
            <button className="setup-secondary" onClick={onExplore}>Use the sample day for now</button>
            <p className="setup-footnote">Your local tasks are untouched. You can return to this guide from Settings.</p>
          </div>
        ) : (
          <>
            <div className="setup-intro">
              <div className="setup-heading">
                <div className="setup-brand"><Logo /><span>Night</span></div>
                <p className="setup-kicker">SET UP KYRA ON THIS MAC</p>
                <h1>Bring your real day into one calm view.</h1>
                <p className="setup-lede">Connect Google once. Kyra will import the work already moving through your inbox and calendar, then keep it current while the app is open.</p>
              </div>
              <div className="setup-access" aria-label="Google access summary">
                <div><span><Mail size={17} /></span><p><strong>Gmail is read-only</strong><small>Inbox + Sent from the last 30 days, up to 500 messages. Kyra cannot send or delete mail.</small></p></div>
                <div><span><CalendarDays size={17} /></span><p><strong>Your primary Calendar</strong><small>Events from 30 days ago through 90 days ahead. Kyra can create and update events when you ask.</small></p></div>
                <div><span><ShieldCheck size={17} /></span><p><strong>Protected on this Mac</strong><small>Provider content is encrypted before SQLite. Refresh tokens and encryption keys stay in macOS Keychain.</small></p></div>
              </div>
            </div>
            <div className="setup-actions">
              <button className="setup-primary" onClick={() => void begin()}>Connect Gmail & Calendar <ArrowRight size={15} /></button>
              <button className="setup-secondary" onClick={onExplore}>Explore with sample data</button>
            </div>
            <p className="setup-footnote">Google will open in your system browser. Kyra uses a desktop OAuth flow and never embeds a client secret.</p>
          </>
        )}
      </section>
    </div>
  );
}

export function ConnectionsSheet({
  status,
  ai,
  busy,
  error,
  aiError,
  onClose,
  onConnect,
  onSync,
  onDisconnect,
  onSaveAi,
  onActivateAi,
  onRunAi,
  onClearAi,
  onDiscoverOllama,
  onOpenSetup,
}: {
  status: GoogleConnectorStatus;
  ai: AiEngineStatus;
  busy: boolean;
  error: string;
  aiError: string;
  onClose: () => void;
  onConnect: () => void;
  onSync: () => void;
  onDisconnect: () => void;
  onSaveAi: (input: SaveAiProviderConfigInput) => Promise<void>;
  onActivateAi: () => Promise<void>;
  onRunAi: () => Promise<void>;
  onClearAi: (provider: AiProvider) => Promise<void>;
  onDiscoverOllama: (baseUrl: string) => Promise<string[]>;
  onOpenSetup: () => void;
}) {
  const connected = status.state !== "disconnected";
  const [provider, setProvider] = useState<AiProvider>(ai.provider ?? "ollama");
  const [model, setModel] = useState(ai.requestedModel ?? "");
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("http://127.0.0.1:11434");
  const [models, setModels] = useState<string[]>([]);

  useEffect(() => {
    if (ai.provider) setProvider(ai.provider);
    if (ai.requestedModel) setModel(ai.requestedModel);
  }, [ai.provider, ai.requestedModel]);

  const save = async () => {
    await onSaveAi({
      provider,
      model,
      apiKey: provider === "ollama" ? undefined : apiKey || undefined,
      baseUrl: provider === "ollama" ? baseUrl : undefined,
    });
    setApiKey("");
  };

  const discover = async () => {
    const discovered = await onDiscoverOllama(baseUrl);
    setModels(discovered);
    if (!model && discovered[0]) setModel(discovered[0]);
  };

  return (
    <div className="connections-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="connections-sheet" aria-label="Settings">
        <header>
          <div><span>KYRA ON THIS MAC</span><h2>Settings</h2></div>
          <button onClick={onClose} aria-label="Close settings"><X size={17} /></button>
        </header>
        <article className="connection-card">
          <div className="connection-provider"><Cloud size={18} /><div><h3>Google</h3><p>{status.accountEmail ?? "Gmail and primary Calendar"}</p></div><span className={`connection-state ${status.state}`}>{connectorLabels[status.state]}</span></div>
          {connected ? (
            <>
              <dl>
                <div><dt>Gmail</dt><dd>{status.gmailMessageCount} messages</dd></div>
                <div><dt>Calendar</dt><dd>{status.calendarEventCount} events</dd></div>
                <div><dt>Last sync</dt><dd>{formatSyncTime(status.lastSyncAt)}</dd></div>
              </dl>
              {(error || status.lastError) && <p className="connection-error">{error || status.lastError}</p>}
              <div className="connection-actions">
                {status.state === "reconnect_required" ? <button className="primary" disabled={busy} onClick={onConnect}>Reconnect</button> : <button className="primary" disabled={busy || status.state === "syncing"} onClick={onSync}><RefreshCw size={14} className={busy ? "spinning" : ""} /> Sync now</button>}
                <button className="disconnect" disabled={busy} onClick={onDisconnect}><Unplug size={14} /> Disconnect</button>
              </div>
            </>
          ) : (
            <>
              <p className="connection-copy">Bring in the last 30 days of Inbox and Sent mail, plus your primary Calendar. The guided setup explains every permission before Google opens.</p>
              {error && <p className="connection-error">{error}</p>}
              <button className="connect-button" disabled={busy} onClick={onOpenSetup}>Open guided setup</button>
              <button className="connection-text-action" disabled={busy} onClick={onConnect}>{busy ? "Waiting for Google…" : "Connect directly"}</button>
              <small>Google opens in your browser. Provider content is encrypted on this Mac.</small>
            </>
          )}
        </article>
        <article className="connection-card ai-connection-card">
          <div className="connection-provider"><Bot size={18} /><div><h3>Intelligence</h3><p>{ai.activatedModel ?? ai.requestedModel ?? "Choose one local or BYOK model"}</p></div><span className={`connection-state ${ai.state}`}>{aiLabels[ai.state]}</span></div>
          <div className="ai-config-grid">
            <label>Provider<select value={provider} onChange={(event) => { setProvider(event.target.value as AiProvider); setModel(""); }}><option value="ollama">Ollama</option><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option></select></label>
            <label>Model<input value={model} list="ollama-models" onChange={(event) => setModel(event.target.value)} placeholder={provider === "ollama" ? "llama3.2" : provider === "openai" ? "gpt-5-mini" : "claude-sonnet-4-5"} /></label>
            <datalist id="ollama-models">{models.map((item) => <option value={item} key={item} />)}</datalist>
            {provider === "ollama" ? (
              <label className="wide">Local URL<div className="input-action"><Server size={13} /><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} /><button type="button" disabled={busy} onClick={() => void discover()}>Discover</button></div></label>
            ) : (
              <label className="wide">API key<div className="input-action"><KeyRound size={13} /><input type="password" autoComplete="off" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={ai.provider === provider ? "Leave blank to keep saved key" : "Stored only in macOS Keychain"} /></div></label>
            )}
          </div>
          {(aiError || ai.lastError) && <p className="connection-error">{aiError || ai.lastError}</p>}
          {ai.provider && <dl className="ai-stats"><div><dt>Queue</dt><dd>{ai.queuedJobs} queued · {ai.failedJobs} failed</dd></div><div><dt>Reviews</dt><dd>{ai.reviewCount}</dd></div><div><dt>Last run</dt><dd>{formatSyncTime(ai.lastRunAt)}</dd></div></dl>}
          <div className="connection-actions ai-actions">
            <button disabled={busy || !model.trim()} onClick={() => void save()}>Save</button>
            <button className="primary" disabled={busy || !ai.provider || ai.state === "testing"} onClick={() => void onActivateAi()}><ShieldCheck size={14} /> {ai.state === "testing" ? "Testing 12 cases…" : "Test & activate"}</button>
            {ai.state === "ready" && <button disabled={busy} onClick={() => void onRunAi()}><RefreshCw size={14} /> Run</button>}
          </div>
          {ai.provider && <button className="clear-provider" disabled={busy} onClick={() => void onClearAi(ai.provider!)}>Remove provider and key</button>}
          <small>Keys are write-only and stay in Keychain. Cloud requests use redacted identities and are not stored by Kyra.</small>
        </article>
      </section>
    </div>
  );
}

function ActivitySheet({
  reviews,
  activity,
  busy,
  error,
  onClose,
  onResolve,
  onRevert,
  onRetry,
}: {
  reviews: AiReview[];
  activity: AiActivity[];
  busy: boolean;
  error: string;
  onClose: () => void;
  onResolve: (id: string, decision: "accept" | "dismiss") => Promise<void>;
  onRevert: (id: string) => Promise<void>;
  onRetry: (id: string) => Promise<void>;
}) {
  return (
    <div className="connections-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="connections-sheet activity-sheet" aria-label="AI activity and reviews">
        <header><div><span>AI ENGINE</span><h2>Activity</h2></div><button onClick={onClose} aria-label="Close activity"><X size={17} /></button></header>
        {error && <p className="connection-error">{error}</p>}
        <div className="activity-scroll">
          <h3 className="activity-section-title">Needs review <b>{reviews.length}</b></h3>
          {reviews.length === 0 && <p className="activity-empty"><ShieldCheck size={15} /> No decisions waiting.</p>}
          {reviews.map((review) => (
            <article className="review-card" key={review.id}>
              <div className="review-title"><AlertCircle size={15} /><div><h4>{review.title}</h4><p>{review.summary}</p></div></div>
              {review.evidence.map((quote, index) => <blockquote key={`${review.id}-${index}`}>{quote}</blockquote>)}
              {review.irreversibleEffects.map((effect) => <p className="irreversible" key={effect}>{effect}</p>)}
              <div className="review-actions"><button disabled={busy} onClick={() => void onResolve(review.id, "dismiss")}>Dismiss</button><button className="primary" disabled={busy} onClick={() => void onResolve(review.id, "accept")}>Accept</button></div>
            </article>
          ))}
          <h3 className="activity-section-title recent">Recent</h3>
          {activity.length === 0 && <p className="activity-empty">No AI actions yet.</p>}
          {activity.map((item) => (
            <article className="activity-card" key={item.id}>
              <div><span>{item.status.replaceAll("_", " ")}</span><h4>{item.title}</h4><p>{item.detail}</p></div>
              {item.irreversibleEffects.map((effect) => <p className="irreversible" key={effect}>{effect}</p>)}
              {item.kind === "job" && item.status !== "succeeded" && <button disabled={busy} onClick={() => void onRetry(item.id)}><RefreshCw size={13} /> Retry</button>}
              {item.canRevert && <button disabled={busy} onClick={() => void onRevert(item.id)}><RotateCcw size={13} /> {item.compensation ? "Compensate" : "Undo"}</button>}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

export default function App() {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [plannerOpen, setPlannerOpen] = useState(false);
  const [connectionsOpen, setConnectionsOpen] = useState(false);
  const [setupOpen, setSetupOpen] = useState(false);
  const [activityOpen, setActivityOpen] = useState(false);
  const [connector, setConnector] = useState<GoogleConnectorStatus>({ state: "disconnected", gmailMessageCount: 0, calendarEventCount: 0 });
  const [aiEngine, setAiEngine] = useState<AiEngineStatus>({ state: "disconnected", queuedJobs: 0, runningJobs: 0, failedJobs: 0, reviewCount: 0 });
  const [reviews, setReviews] = useState<AiReview[]>([]);
  const [activityItems, setActivityItems] = useState<AiActivity[]>([]);
  const [connectorBusy, setConnectorBusy] = useState(false);
  const [aiBusy, setAiBusy] = useState(false);
  const [connectorError, setConnectorError] = useState("");
  const [aiError, setAiError] = useState("");
  const [activityError, setActivityError] = useState("");
  const [error, setError] = useState("");
  const setupEvaluated = useRef(false);

  useEffect(() => {
    const load = () => {
      void Promise.all([getDashboard(), getGoogleConnectorStatus(), getAiEngineStatus()])
        .then(([nextDashboard, nextConnector, nextAi]) => {
          setDashboard(nextDashboard);
          setConnector(nextConnector);
          setAiEngine(nextAi);
          if (!setupEvaluated.current) {
            setupEvaluated.current = true;
            setSetupOpen(shouldShowSetup(nextConnector, readSetupPreference()));
          }
        })
        .catch((cause) => setError(String(cause)));
    };
    load();
    const refresh = window.setInterval(load, 30_000);
    return () => window.clearInterval(refresh);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const refresh = () => {
      if (disposed) return;
      void Promise.all([getDashboard(), getAiEngineStatus(), listAiReviews(), listAiActivity()])
        .then(([nextDashboard, nextAi, nextReviews, nextActivity]) => {
          if (disposed) return;
          setDashboard(nextDashboard);
          setAiEngine(nextAi);
          setReviews(nextReviews);
          setActivityItems(nextActivity);
        })
        .catch((cause) => setActivityError(cause instanceof Error ? cause.message : String(cause)));
    };
    void Promise.all(["ai-engine-state-changed", "dashboard-invalidated", "ai-review-changed", "ai-action-completed"].map(async (event) => {
      const unlisten = await listen(event, refresh);
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    }));
    return () => { disposed = true; unlisteners.forEach((unlisten) => unlisten()); };
  }, []);

  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (activityOpen) setActivityOpen(false);
      else if (setupOpen && !connectorBusy) setSetupOpen(false);
      else if (connectionsOpen) setConnectionsOpen(false);
      else if (plannerOpen) setPlannerOpen(false);
      else void hideOverlay();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [activityOpen, connectorBusy, connectionsOpen, plannerOpen, setupOpen]);

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
    if (isTauri()) {
      if (connector.state !== "connected") throw new Error("Connect Google Calendar before using /cal.");
      await mutateGoogleCalendar({
        action: "create",
        operationId: crypto.randomUUID(),
        event: {
          title,
          when: { kind: "timed", startAt, endAt, timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone },
          attendees: [],
          recurrence: [],
          sendUpdates: "all",
        },
      });
      setDashboard(await getDashboard());
    } else {
      const block = await createCalendarBlock(title, startAt, endAt);
      setDashboard((current) => current ? { ...current, calendarBlocks: [...current.calendarBlocks, block] } : current);
    }
  };

  const refreshNativeState = async () => {
    const [nextDashboard, nextConnector, nextAi] = await Promise.all([getDashboard(), getGoogleConnectorStatus(), getAiEngineStatus()]);
    setDashboard(nextDashboard);
    setConnector(nextConnector);
    setAiEngine(nextAi);
  };

  const refreshAiActivity = async () => {
    const [nextAi, nextReviews, nextActivity] = await Promise.all([getAiEngineStatus(), listAiReviews(), listAiActivity()]);
    setAiEngine(nextAi);
    setReviews(nextReviews);
    setActivityItems(nextActivity);
  };

  const connect = async () => {
    setConnectorBusy(true);
    setConnectorError("");
    setConnector((current) => ({ ...current, state: "connecting" }));
    const statusPoll = window.setInterval(() => {
      void getGoogleConnectorStatus().then((next) => {
        if (next.state === "syncing" || next.state === "connecting") setConnector(next);
      }).catch(() => undefined);
    }, 750);
    try {
      const next = await connectGoogle();
      setConnector(next);
      writeSetupPreference("completed");
      await refreshNativeState();
    } catch (cause) {
      setConnectorError(cause instanceof Error ? cause.message : String(cause));
      setConnector(await getGoogleConnectorStatus());
    } finally {
      window.clearInterval(statusPoll);
      setConnectorBusy(false);
    }
  };

  const syncConnector = async () => {
    setConnectorBusy(true);
    setConnectorError("");
    setConnector((current) => ({ ...current, state: "syncing" }));
    try {
      await syncGoogleNow();
      await refreshNativeState();
    } catch (cause) {
      setConnectorError(cause instanceof Error ? cause.message : String(cause));
      setConnector(await getGoogleConnectorStatus());
    } finally {
      setConnectorBusy(false);
    }
  };

  const disconnect = async () => {
    if (!window.confirm("Disconnect Google and remove its synchronized data from this Mac?")) return;
    setConnectorBusy(true);
    setConnectorError("");
    try {
      setConnector(await disconnectGoogle());
      await refreshNativeState();
    } catch (cause) {
      setConnectorError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setConnectorBusy(false);
    }
  };

  const saveAi = async (input: SaveAiProviderConfigInput) => {
    setAiBusy(true);
    setAiError("");
    try {
      setAiEngine(await saveAiProviderConfig(input));
    } catch (cause) {
      setAiError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    } finally {
      setAiBusy(false);
    }
  };

  const activateAi = async () => {
    setAiBusy(true);
    setAiError("");
    setAiEngine((current) => ({ ...current, state: "testing" }));
    try {
      const report = await testAiProvider();
      if (!report.passed) throw new Error("This model did not pass Kyra’s 12-case safety activation.");
      await refreshAiActivity();
    } catch (cause) {
      setAiError(cause instanceof Error ? cause.message : String(cause));
      setAiEngine(await getAiEngineStatus());
    } finally {
      setAiBusy(false);
    }
  };

  const runAi = async () => {
    setAiBusy(true);
    setAiError("");
    try {
      setAiEngine(await runAiNow());
      await Promise.all([refreshNativeState(), refreshAiActivity()]);
    } catch (cause) {
      setAiError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setAiBusy(false);
    }
  };

  const clearAi = async (provider: AiProvider) => {
    setAiBusy(true);
    setAiError("");
    try {
      setAiEngine(await clearAiProvider(provider));
    } catch (cause) {
      setAiError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setAiBusy(false);
    }
  };

  const discoverOllama = async (baseUrl: string) => {
    setAiBusy(true);
    setAiError("");
    try {
      return (await listOllamaModels(baseUrl)).map((item) => item.name);
    } catch (cause) {
      setAiError(cause instanceof Error ? cause.message : String(cause));
      return [];
    } finally {
      setAiBusy(false);
    }
  };

  const naturalCommand = async (text: string, sessionId?: string) => {
    const result = await executeAiCommand(text, sessionId);
    await Promise.all([refreshNativeState(), refreshAiActivity()]);
    return result;
  };

  const resolveReview = async (id: string, decision: "accept" | "dismiss") => {
    setAiBusy(true);
    setActivityError("");
    try {
      await resolveAiReview(id, decision);
      await Promise.all([refreshNativeState(), refreshAiActivity()]);
    } catch (cause) {
      setActivityError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setAiBusy(false);
    }
  };

  const revertAction = async (id: string) => {
    setAiBusy(true);
    setActivityError("");
    try {
      const result = await revertAiAction(id);
      setActivityError(result.message);
      await Promise.all([refreshNativeState(), refreshAiActivity()]);
    } catch (cause) {
      setActivityError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setAiBusy(false);
    }
  };

  const retryJob = async (id: string) => {
    setAiBusy(true);
    setActivityError("");
    try {
      setAiEngine(await retryAiJob(id));
      await runAi();
    } catch (cause) {
      setActivityError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setAiBusy(false);
    }
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

  const systemState = aiEngine.reviewCount > 0 ? "review" : aiEngine.state === "ready" ? connector.state : aiEngine.state;
  const systemTitle = aiEngine.reviewCount > 0 ? `${aiEngine.reviewCount} AI review${aiEngine.reviewCount === 1 ? "" : "s"} waiting` : `Google: ${connectorLabels[connector.state]} · AI: ${aiLabels[aiEngine.state]}`;

  const finishSetup = () => {
    writeSetupPreference("completed");
    setSetupOpen(false);
  };

  const exploreSample = () => {
    writeSetupPreference("skipped");
    setSetupOpen(false);
  };

  return (
    <main className="app-shell">
      <Timeline blocks={dashboard.calendarBlocks} now={dashboard.now} onExpand={() => setPlannerOpen(true)} />
      <section className="night-panel">
        <div className="night-content">
          <div className="night-title"><Logo /><span>Night</span></div>
          <p>{dashboard.briefing}</p>
        </div>
        <CommandPalette onTask={addTask} onCalendar={addCalendar} onNatural={naturalCommand} />
      </section>
      <Loops loops={dashboard.openLoops} onComplete={complete} />
      <span className="loop-count">{37 + Math.max(0, visibleLoops.length - 5)}</span>
      <div className="utility-actions">
        <button className="activity-trigger" aria-label="Open AI activity" onClick={() => { setActivityOpen(true); void refreshAiActivity(); }}><Activity size={14} />{aiEngine.reviewCount > 0 && <span>{aiEngine.reviewCount}</span>}</button>
        <button className={`settings-trigger ${systemState}`} title={systemTitle} aria-label="Open settings" onClick={() => setConnectionsOpen(true)}><i /><Settings size={13} /><span>{connector.state === "disconnected" ? "Set up" : connector.state === "reconnect_required" ? "Reconnect" : "Settings"}</span></button>
      </div>
      {plannerOpen && <Planner blocks={dashboard.calendarBlocks} showDemo={connector.state === "disconnected"} onClose={() => setPlannerOpen(false)} />}
      {connectionsOpen && <ConnectionsSheet status={connector} ai={aiEngine} busy={connectorBusy || aiBusy} error={connectorError} aiError={aiError} onClose={() => setConnectionsOpen(false)} onConnect={() => void connect()} onSync={() => void syncConnector()} onDisconnect={() => void disconnect()} onSaveAi={saveAi} onActivateAi={activateAi} onRunAi={runAi} onClearAi={clearAi} onDiscoverOllama={discoverOllama} onOpenSetup={() => { setConnectionsOpen(false); setConnectorError(""); setSetupOpen(true); }} />}
      {activityOpen && <ActivitySheet reviews={reviews} activity={activityItems} busy={aiBusy} error={activityError} onClose={() => setActivityOpen(false)} onResolve={resolveReview} onRevert={revertAction} onRetry={retryJob} />}
      {setupOpen && <SetupFlow status={connector} busy={connectorBusy} error={connectorError} onClose={() => setSetupOpen(false)} onConnect={connect} onFinish={finishSetup} onExplore={exploreSample} />}
    </main>
  );
}
