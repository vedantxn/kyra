-- Bring previously seeded local demo databases up to the visual walkthrough state.
-- A brand-new database is still populated by the Rust seed after migrations run.
UPDATE open_loops
SET summary = 'You followed up asking why you haven''t gotten any edited videos, and Manish said his editor just started and will send a few by morning.',
    status = 'waiting',
    priority = 95,
    version = 1
WHERE id = 'waiting-manish';

UPDATE open_loops
SET summary = 'You pushed Ayush hard for the write-up and asked him to send it by tomorrow; he said he''d send it and add more new stuff, but it hasn''t arrived yet.',
    status = 'waiting',
    priority = 90,
    version = 1
WHERE id = 'waiting-ayush';

UPDATE open_loops
SET title = 'Print, sign, mail the 83(b) form via USPS and send Phalanshu the receipt',
    summary = 'You told Phalanshu you''d do the 83(b) mailing, and he reminded you to keep the USPS receipt as proof — this is still pending on your end.',
    status = 'open',
    priority = 86,
    version = 1
WHERE id = 'mail-receipt';

UPDATE open_loops
SET title = 'Update RC on how the pitch/meeting went',
    summary = 'RC asked how it went and you only said it''s in 20 mins — you still haven''t told him the outcome.',
    status = 'open',
    priority = 78,
    version = 1
WHERE id = 'update-rc';

INSERT OR IGNORE INTO open_loops (
  id, title, summary, owner, status, priority, due_at, version, created_at, updated_at
)
SELECT
  'samarth-sign-doc',
  'Samarth to sign the doc tonight',
  'You asked Samarth to sign and he said he''d do it tonight, so you''re waiting on him.',
  'them',
  'waiting',
  82,
  NULL,
  1,
  datetime('now'),
  datetime('now')
WHERE EXISTS (SELECT 1 FROM open_loops WHERE id = 'waiting-manish');

INSERT OR IGNORE INTO evidence (
  id, loop_id, source_kind, source_label, excerpt, occurred_at
)
SELECT
  'e5',
  'samarth-sign-doc',
  'fixture_message',
  'Message with Samarth',
  'I''ll sign it tonight.',
  datetime('now', '-1 day')
WHERE EXISTS (SELECT 1 FROM open_loops WHERE id = 'samarth-sign-doc');
