ALTER TABLE open_loops ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'
  CHECK (origin IN ('demo', 'local', 'google'));
UPDATE open_loops SET origin = 'demo'
WHERE id IN ('waiting-manish', 'waiting-ayush', 'mail-receipt', 'samarth-sign-doc', 'update-rc');

ALTER TABLE calendar_blocks ADD COLUMN origin TEXT NOT NULL DEFAULT 'local'
  CHECK (origin IN ('demo', 'local', 'google'));
ALTER TABLE calendar_blocks ADD COLUMN external_id TEXT;
ALTER TABLE calendar_blocks ADD COLUMN etag TEXT;
UPDATE calendar_blocks SET origin = 'demo'
WHERE id IN ('sleep', 'gym', 'meeting', 'deep-work');
CREATE UNIQUE INDEX IF NOT EXISTS idx_calendar_blocks_google_external
  ON calendar_blocks(external_id) WHERE external_id IS NOT NULL;

CREATE TABLE connector_accounts (
  id TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL CHECK (provider = 'google'),
  state TEXT NOT NULL CHECK (state IN ('connecting', 'syncing', 'connected', 'reconnect_required', 'error')),
  email_nonce BLOB NOT NULL,
  email_ciphertext BLOB NOT NULL,
  granted_scopes TEXT NOT NULL,
  gmail_history_id TEXT,
  calendar_sync_token TEXT,
  calendar_window_anchor TEXT,
  last_sync_at TEXT,
  next_sync_at TEXT,
  retry_count INTEGER NOT NULL DEFAULT 0,
  last_error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE provider_items (
  id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES connector_accounts(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('gmail_message', 'calendar_event')),
  external_id TEXT NOT NULL,
  thread_id TEXT,
  etag TEXT,
  occurred_at TEXT,
  starts_at TEXT,
  ends_at TEXT,
  status TEXT,
  nonce BLOB NOT NULL,
  ciphertext BLOB NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(account_id, kind, external_id)
);
CREATE INDEX idx_provider_items_account_kind_time
  ON provider_items(account_id, kind, occurred_at DESC);
CREATE INDEX idx_provider_calendar_window
  ON provider_items(account_id, kind, starts_at, ends_at);

CREATE TABLE connector_mutations (
  operation_id TEXT PRIMARY KEY NOT NULL,
  account_id TEXT NOT NULL REFERENCES connector_accounts(id) ON DELETE CASCADE,
  action TEXT NOT NULL CHECK (action IN ('create', 'update', 'delete')),
  target_external_id TEXT,
  payload_hash TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'succeeded', 'conflict', 'failed')),
  result_external_id TEXT,
  last_error_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
