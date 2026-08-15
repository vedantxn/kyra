PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS open_loops (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 240),
  summary TEXT NOT NULL DEFAULT '',
  owner TEXT NOT NULL CHECK (owner IN ('me', 'them')),
  status TEXT NOT NULL CHECK (status IN ('open', 'waiting', 'done', 'dismissed')),
  priority INTEGER NOT NULL DEFAULT 50 CHECK (priority BETWEEN 0 AND 100),
  due_at TEXT,
  version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_open_loops_active_priority
  ON open_loops(status, priority DESC, updated_at DESC);

CREATE TABLE IF NOT EXISTS evidence (
  id TEXT PRIMARY KEY NOT NULL,
  loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL,
  source_label TEXT NOT NULL,
  excerpt TEXT NOT NULL,
  occurred_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_loop ON evidence(loop_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS calendar_blocks (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 240),
  start_at TEXT NOT NULL,
  end_at TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('meeting', 'execution', 'routine')),
  color TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_calendar_blocks_start ON calendar_blocks(start_at);

CREATE TABLE IF NOT EXISTS loop_transitions (
  id TEXT PRIMARY KEY NOT NULL,
  loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  from_status TEXT,
  to_status TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL
);
