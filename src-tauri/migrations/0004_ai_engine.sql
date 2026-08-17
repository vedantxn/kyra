PRAGMA foreign_keys = ON;

CREATE TABLE app_meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

ALTER TABLE open_loops ADD COLUMN lifecycle TEXT NOT NULL DEFAULT 'active'
  CHECK (lifecycle IN ('active', 'resolved', 'dismissed'));
ALTER TABLE open_loops ADD COLUMN ownership TEXT NOT NULL DEFAULT 'me'
  CHECK (ownership IN ('me', 'other', 'shared', 'unknown'));
ALTER TABLE open_loops ADD COLUMN payload_nonce BLOB;
ALTER TABLE open_loops ADD COLUMN payload_ciphertext BLOB;
ALTER TABLE open_loops ADD COLUMN payload_migrated INTEGER NOT NULL DEFAULT 0
  CHECK (payload_migrated IN (0, 1));
UPDATE open_loops
SET lifecycle = CASE status
    WHEN 'done' THEN 'resolved'
    WHEN 'dismissed' THEN 'dismissed'
    ELSE 'active'
  END,
  ownership = CASE owner WHEN 'them' THEN 'other' ELSE 'me' END;
CREATE INDEX idx_open_loops_lifecycle_priority
  ON open_loops(lifecycle, priority DESC, updated_at DESC);

ALTER TABLE evidence ADD COLUMN source_revision_id TEXT;
ALTER TABLE evidence ADD COLUMN document_hash TEXT;
ALTER TABLE evidence ADD COLUMN start_offset INTEGER;
ALTER TABLE evidence ADD COLUMN end_offset INTEGER;
ALTER TABLE evidence ADD COLUMN quote_hash TEXT;
ALTER TABLE evidence ADD COLUMN payload_nonce BLOB;
ALTER TABLE evidence ADD COLUMN payload_ciphertext BLOB;
ALTER TABLE evidence ADD COLUMN payload_migrated INTEGER NOT NULL DEFAULT 0
  CHECK (payload_migrated IN (0, 1));

ALTER TABLE loop_transitions ADD COLUMN payload_nonce BLOB;
ALTER TABLE loop_transitions ADD COLUMN payload_ciphertext BLOB;
ALTER TABLE loop_transitions ADD COLUMN payload_migrated INTEGER NOT NULL DEFAULT 0
  CHECK (payload_migrated IN (0, 1));

ALTER TABLE connector_accounts ADD COLUMN generation INTEGER NOT NULL DEFAULT 1;
ALTER TABLE provider_items ADD COLUMN latest_revision_id TEXT;
ALTER TABLE provider_items ADD COLUMN ingest_generation_id TEXT;

CREATE TABLE ingest_generations (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES connector_accounts(id) ON DELETE CASCADE,
  account_generation INTEGER NOT NULL,
  source_kind TEXT NOT NULL CHECK (source_kind IN ('gmail_message', 'calendar_event')),
  status TEXT NOT NULL CHECK (status IN ('pending', 'complete', 'superseded', 'failed')),
  expected_items INTEGER NOT NULL DEFAULT 0,
  committed_items INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  completed_at TEXT
);
CREATE INDEX idx_ingest_generations_account_status
  ON ingest_generations(account_id, status, created_at DESC);

CREATE TABLE provider_item_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES connector_accounts(id) ON DELETE CASCADE,
  account_generation INTEGER NOT NULL,
  ingest_generation_id TEXT REFERENCES ingest_generations(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK (kind IN ('gmail_message', 'calendar_event')),
  external_id TEXT NOT NULL,
  thread_id TEXT,
  provider_version TEXT,
  content_hash TEXT NOT NULL,
  tombstone INTEGER NOT NULL DEFAULT 0 CHECK (tombstone IN (0, 1)),
  occurred_at TEXT,
  nonce BLOB NOT NULL,
  ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(account_id, kind, external_id, content_hash, tombstone)
);
CREATE INDEX idx_provider_revisions_thread
  ON provider_item_revisions(account_id, kind, thread_id, created_at);
CREATE INDEX idx_provider_revisions_source
  ON provider_item_revisions(account_id, kind, external_id, created_at DESC);

CREATE TABLE ai_provider_configs (
  provider TEXT PRIMARY KEY NOT NULL CHECK (provider IN ('openai', 'anthropic', 'ollama')),
  model TEXT NOT NULL,
  base_url TEXT,
  selected INTEGER NOT NULL DEFAULT 0 CHECK (selected IN (0, 1)),
  config_generation INTEGER NOT NULL DEFAULT 1,
  credential_generation INTEGER NOT NULL DEFAULT 1,
  activation_fingerprint TEXT,
  activated_model TEXT,
  activated_at TEXT,
  activation_expires_at TEXT,
  state TEXT NOT NULL DEFAULT 'disconnected'
    CHECK (state IN ('disconnected', 'testing', 'ready', 'running', 'paused', 'blocked', 'error')),
  last_error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_ai_provider_selected
  ON ai_provider_configs(selected) WHERE selected = 1;

CREATE TABLE ai_jobs (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('extract_thread', 'reconcile_generation', 'compose_briefing')),
  account_id TEXT REFERENCES connector_accounts(id) ON DELETE CASCADE,
  account_generation INTEGER,
  ingest_generation_id TEXT REFERENCES ingest_generations(id) ON DELETE CASCADE,
  source_revision_id TEXT REFERENCES provider_item_revisions(id) ON DELETE CASCADE,
  activation_fingerprint TEXT,
  idempotency_key TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('queued', 'leased', 'succeeded', 'failed', 'dead_letter', 'cancelled')),
  priority INTEGER NOT NULL DEFAULT 50,
  attempt INTEGER NOT NULL DEFAULT 0,
  not_before TEXT NOT NULL,
  lease_owner TEXT,
  lease_token TEXT,
  leased_until TEXT,
  heartbeat_at TEXT,
  payload_nonce BLOB,
  payload_ciphertext BLOB,
  last_error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_ai_jobs_claim
  ON ai_jobs(status, not_before, priority DESC, created_at);
CREATE INDEX idx_ai_jobs_generation
  ON ai_jobs(ingest_generation_id, kind, status);

CREATE TABLE ai_model_runs (
  id TEXT PRIMARY KEY NOT NULL,
  job_id TEXT REFERENCES ai_jobs(id) ON DELETE SET NULL,
  provider TEXT NOT NULL,
  requested_model TEXT NOT NULL,
  resolved_model TEXT,
  activation_fingerprint TEXT NOT NULL,
  prompt_version TEXT NOT NULL,
  schema_version TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  output_hash TEXT,
  input_units INTEGER,
  output_units INTEGER,
  latency_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected', 'refused', 'error')),
  error_code TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_ai_model_runs_created ON ai_model_runs(created_at DESC);

CREATE TABLE ai_people (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT REFERENCES connector_accounts(id) ON DELETE CASCADE,
  stable_hash TEXT NOT NULL UNIQUE,
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE ai_person_aliases (
  id TEXT PRIMARY KEY NOT NULL,
  person_id TEXT NOT NULL REFERENCES ai_people(id) ON DELETE CASCADE,
  alias_hash TEXT NOT NULL,
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(person_id, alias_hash)
);

CREATE TABLE loop_derivations (
  id TEXT PRIMARY KEY NOT NULL,
  loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  field_name TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('user', 'command', 'google')),
  source_revision_id TEXT REFERENCES provider_item_revisions(id) ON DELETE CASCADE,
  model_run_id TEXT REFERENCES ai_model_runs(id) ON DELETE SET NULL,
  active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  value_hash TEXT NOT NULL,
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_loop_derivations_loop_field
  ON loop_derivations(loop_id, field_name, active, created_at DESC);

CREATE TABLE loop_calendar_links (
  loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  calendar_external_id TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('user', 'command', 'google')),
  created_at TEXT NOT NULL,
  PRIMARY KEY(loop_id, calendar_external_id)
);

CREATE TABLE loop_relations (
  from_loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  to_loop_id TEXT NOT NULL REFERENCES open_loops(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind = 'supersedes'),
  created_at TEXT NOT NULL,
  PRIMARY KEY(from_loop_id, to_loop_id, kind)
);

CREATE TABLE ai_reviews (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'dismissed', 'superseded')),
  account_id TEXT REFERENCES connector_accounts(id) ON DELETE CASCADE,
  account_generation INTEGER,
  source_revision_id TEXT REFERENCES provider_item_revisions(id) ON DELETE CASCADE,
  model_run_id TEXT REFERENCES ai_model_runs(id) ON DELETE SET NULL,
  target_loop_id TEXT REFERENCES open_loops(id) ON DELETE CASCADE,
  target_event_id TEXT,
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  resolved_at TEXT
);
CREATE INDEX idx_ai_reviews_status_created ON ai_reviews(status, created_at DESC);

CREATE TABLE ai_actions (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('succeeded', 'reverted', 'compensated', 'conflict', 'failed')),
  account_id TEXT REFERENCES connector_accounts(id) ON DELETE CASCADE,
  account_generation INTEGER,
  model_run_id TEXT REFERENCES ai_model_runs(id) ON DELETE SET NULL,
  target_loop_id TEXT REFERENCES open_loops(id) ON DELETE SET NULL,
  target_event_id TEXT,
  resulting_version TEXT,
  irreversible_effects INTEGER NOT NULL DEFAULT 0 CHECK (irreversible_effects IN (0, 1)),
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  reverted_at TEXT
);
CREATE INDEX idx_ai_actions_created ON ai_actions(created_at DESC);

CREATE TABLE ai_command_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('waiting', 'consumed', 'expired', 'cancelled')),
  clarification_count INTEGER NOT NULL DEFAULT 0,
  time_zone TEXT NOT NULL,
  anchored_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  payload_nonce BLOB NOT NULL,
  payload_ciphertext BLOB NOT NULL,
  result_nonce BLOB,
  result_ciphertext BLOB,
  created_at TEXT NOT NULL,
  consumed_at TEXT
);

CREATE TABLE ai_secret_cleanup (
  id TEXT PRIMARY KEY NOT NULL,
  service TEXT NOT NULL,
  account TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'complete')),
  attempt INTEGER NOT NULL DEFAULT 0,
  last_error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
