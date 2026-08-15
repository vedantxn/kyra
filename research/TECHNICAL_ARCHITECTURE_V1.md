# Kyra Reconstruction: Technical Architecture V1

**Status:** Proposed implementation architecture

**Date:** 15 August 2026

**Target:** A software-first macOS proof that demonstrates the complete open-loop product cycle

**Source of truth:** [Current product and interface brief](./current-interface/README.md)

## 1. Executive decision

V1 will be a **local-first Tauri 2 desktop application implemented as a modular monolith**.

The application will contain five clear modules inside one signed macOS application bundle:

1. the ambient desktop shell and trusted IPC boundary;
2. the local source connectors and sync worker;
3. the evidence-backed open-loop engine;
4. the approval and action engine;
5. the React interface for the timeline, Night briefing, open loops, and command palette.

SQLite will be the durable source of truth. Google Calendar and Gmail will be the first real connectors. A fixture-backed message connector will represent WhatsApp-shaped conversations in the demo because a reliable, policy-safe personal WhatsApp API is not available for this V1. Model calls will go directly from the desktop app to a provider selected by the user, using the user's key. There will be no Kyra cloud backend in V1.

This is intentionally a small system. The technical proof is not that we can deploy many services. The proof is that we can maintain an accurate, inspectable model of a person's unfinished commitments and keep it reconciled as new evidence arrives.

## 2. Product contract

The product has one primary responsibility:

> Convert permitted communication and calendar activity into an accurate, evidence-backed, continuously reconciled model of what the user owes, what others owe the user, what is scheduled, and what needs attention now.

The critical V1 loop is:

    observe source activity
            |
            v
    normalize events
            |
            v
    extract commitment candidates
            |
            v
    verify evidence and deduplicate
            |
            v
    maintain open-loop state
            |
            v
    prioritize and brief
            |
            v
    user corrects, schedules, or resolves
            |
            v
    later source activity reconciles the loop

The translucent interface is a delivery surface for this loop, not the architecture's center of gravity.

## 3. What exists today

The repository currently contains:

- the project mission and motivation;
- a landing-page and screenshot-based product brief;
- the supplied visual reference material.

There is no application runtime, schema, connector, model pipeline, test suite, signing setup, or deployment system yet. This document therefore defines a greenfield implementation and does not claim to describe Kyra's private architecture.

## 4. V1 scope

### 4.1 In scope

- A signed macOS desktop application.
- Global Command+K activation with a visible fallback when the shortcut is unavailable.
- A translucent full-screen overlay that dismisses with Escape or click-away.
- A today timeline and editable three-day planner.
- Local task creation through /task.
- Calendar creation through /cal with a preview before provider writes.
- Google Calendar read and write integration.
- Gmail read-only integration.
- A deterministic fixture connector for WhatsApp-shaped conversations.
- Evidence-backed commitment extraction.
- Separate ownership and lifecycle state for every open loop.
- Deduplication and reconciliation when later evidence arrives.
- A deterministic priority score and concise Night briefing.
- User correction, dismissal, scheduling, reopening, and resolution.
- Local audit history for every inferred and user-triggered transition.
- Direct bring-your-own-key model access with local redaction and pseudonymization.
- Offline use of previously synchronized data.
- Packaging, code signing, notarization, and a repeatable demo build.

### 4.2 Explicitly not in scope

- Wearable or custom hardware.
- iOS, Android, Windows, or Linux.
- A hosted multi-user backend.
- Team workspaces or shared task ownership.
- Direct personal WhatsApp account automation.
- Sending email or messages on the user's behalf.
- Fully autonomous consequential actions.
- Attachment parsing beyond plain-text metadata and a small allowlist of text formats.
- Every calendar, mail, or messaging provider.
- A generic assistant unrelated to open loops.
- Public Google OAuth launch approval during the build sprint. It remains a launch gate.

## 5. Architecture principles

1. **Evidence before inference.** Every inferred loop must point to exact source evidence.
2. **Ownership is not status.** "On me" and "waiting on someone" describe responsibility; open, scheduled, and resolved describe lifecycle.
3. **User corrections dominate.** A user decision cannot be silently overwritten by a later model run.
4. **Consequential actions require approval.** Calendar writes receive a preview and confirmation. V1 does not send messages.
5. **Local data is authoritative.** Providers supply events, but the local database owns the derived commitment model and audit history.
6. **Deterministic code controls state.** Models propose structured facts. They do not call tools or mutate state directly.
7. **Repeated work must be safe.** Idempotency, meaning that a repeated request produces the same single logical result, is required across sync, extraction, and provider writes.
8. **Useful failure is visible.** Stale data, a disconnected connector, or an unavailable model is shown in the interface instead of being hidden.
9. **One deployable application, strong internal boundaries.** A modular monolith keeps V1 fast to build while preserving seams that can later be separated if real load requires it.

## 6. Recommended stack

| Layer | Choice | Reason |
|---|---|---|
| Desktop shell | Tauri 2, current stable versions locked in Cargo.lock | Uses macOS WKWebView, supports global shortcuts and transparent windows, and avoids bundling a browser engine |
| Privileged core | Rust stable + Tokio | Owns secrets, network access, SQLite, background workers, and state mutation with a memory-safe native boundary |
| Frontend | React + TypeScript + Vite | Fast UI iteration and a clear component model |
| Calendar | FullCalendar React with time-grid and interaction packages | Three-day time grid, selection, drag, and resize are standard capabilities |
| Styling | CSS Modules plus design tokens | Enough isolation without adding a UI framework that fights the reference design |
| Validation | Serde in Rust and Zod in TypeScript | Both sides reject malformed command, provider, and model payloads |
| Local database | SQLite through SQLx inside the Rust core | Explicit Rust-owned transactions, compile-time typed access where practical, and no database capability exposed to the webview |
| Migrations | Numbered SQL migrations applied by SQLx | The schema remains explicit and reviewable |
| HTTP and model access | Rust provider adapters through reqwest, with an OpenAI-compatible first implementation and deterministic fake | Keeps provider credentials and network authority outside the webview |
| Rust tests | cargo test + property tests for critical domain rules | Exercises state transitions, persistence, security, jobs, and connector behavior |
| Frontend tests | Vitest | Fast React and TypeScript tests with V8 coverage |
| Desktop end-to-end tests | WebdriverIO with @wdio/tauri-service | Supports packaged Tauri automation on macOS and command mocking |
| Packaging | Tauri CLI and official updater plugin | Produces signed DMG and signed updater artifacts without bundling Chromium |
| Package managers | pnpm + Cargo | Reproducible JavaScript and Rust dependency management |

### 6.1 Why Tauri for V1

Kyra is an ambient application that may stay open all day. Installer size, idle memory, launch time, and native macOS behavior therefore matter more than keeping the entire implementation in TypeScript.

Tauri reuses macOS WKWebView instead of bundling a browser engine. Tauri 2 also provides official support for the capabilities Kyra needs: global shortcuts, transparent and always-on-top windows, system-browser opening, single-instance behavior, logging, signed updates, and fine-grained frontend permissions.

The tradeoff is a Rust and TypeScript boundary. V1 accepts that cost because the boundary is useful: the React webview remains presentation-only while Rust owns Google access, model requests, SQLite, encryption, jobs, and every external mutation. macOS is the only V1 target, so cross-platform WebView differences do not complicate the initial release.

The app will still be profiled rather than described as lightweight by assumption. The performance budgets in this document are release gates.

### 6.2 Why no backend

A backend would immediately create account, tenancy, secret management, data residency, and operational obligations without improving the V1 proof. Gmail and Calendar can synchronize directly to the installed application. Model calls can also originate locally with the user's key.

The absence of a backend means V1 will use polling rather than Google push notifications, since push delivery requires an internet-reachable receiver. This tradeoff is acceptable for a personal awareness tool at demo scale.

## 7. System context

    +----------------------+       +----------------------+
    | Gmail API           |       | Google Calendar API  |
    | read-only           |       | read and approved    |
    | incremental history |       | writes               |
    +----------+-----------+       +----------+-----------+
               |                              |
               +---------------+--------------+
                               |
                               v
    +-----------------------------------------------------+
    | Kyra macOS application                             |
    |                                                     |
    |  +------------------+   +------------------------+  |
    |  | Connector layer  |-->| Normalize and sync     |  |
    |  +------------------+   +------------+-----------+  |
    |                                      |              |
    |                                      v              |
    |  +------------------+   +------------------------+  |
    |  | Model adapter    |<->| Open-loop engine       |  |
    |  | no tools         |   | evidence + state       |  |
    |  +------------------+   +------------+-----------+  |
    |                                      |              |
    |                                      v              |
    |  +------------------+   +------------------------+  |
    |  | SQLite           |<->| Application services   |  |
    |  | local truth      |   | commands + approvals   |  |
    |  +------------------+   +------------+-----------+  |
    |                                      | Tauri command|
    |                                      v              |
    |                           +------------------------+ |
    |                           | React webview          | |
    |                           | overlay + planner      | |
    |                           +------------------------+ |
    +-----------------------------------------------------+
                               |
                               v
                    +----------------------+
                    | User-selected model  |
                    | provider, direct API |
                    +----------------------+

The fixture message connector enters through the same connector interface as Gmail. It is never labeled as live WhatsApp integration.

## 8. Runtime and trust boundaries

### 8.1 Rust core

The Rust core is privileged and owns:

- the global shortcut and overlay window lifecycle;
- OAuth and provider API clients;
- the SQLite connection and migrations;
- connector workers and the persisted job queue;
- model-provider requests;
- encryption and the macOS Keychain adapter;
- file export, diagnostic collection, and update checks;
- all application services that mutate state.

Async connector and model work runs on Tokio tasks. SQLite mutation still passes through a bounded repository service so long network operations never hold database transactions and the macOS UI thread never performs blocking work.

### 8.2 Tauri command boundary

The webview calls a small versioned set of Tauri commands through invoke. The Rust build declares an application command manifest, generates allow permissions only for those commands, and grants those permissions only to the bundled main window capability. The application does not expose the SQL, HTTP, shell, filesystem, or Stronghold plugins directly to frontend JavaScript.

Example API groups:

- dashboard.get(date)
- loops.list(filter), loops.get(id), loops.correct(id, correction)
- loops.setStatus(id, targetStatus, expectedVersion)
- calendar.preview(command), calendar.confirm(actionId)
- calendar.move(blockId, start, end, expectedVersion)
- calendar.resize(blockId, start, end, expectedVersion)
- commands.parse(text), commands.confirm(draftId)
- connectors.list(), connectors.connect(type), connectors.sync(id)
- settings.get(), settings.update(patch)

Each request must pass its Zod schema in TypeScript and deserialize into a Serde type in Rust. Rust then performs domain validation before executing the command. Responses follow the same versioned contract and return typed error codes rather than internal error strings.

Only bundled application content may invoke commands. Remote origins are not granted capabilities. External URLs always open in the system browser after allowlist validation.

### 8.3 Webview frontend

The React webview is treated as untrusted presentation code. It can request application operations but cannot:

- read OAuth refresh tokens or model keys;
- issue network requests to providers;
- access the filesystem;
- execute arbitrary shell commands;
- access SQLite;
- decide whether an external action is approved.

This boundary limits the impact of a frontend bug or injected source content.

## 9. Proposed source layout

    src/
      app/
      features/
        overlay/
        timeline/
        planner/
        briefing/
        loops/
        commands/
        onboarding/
      components/
      contracts/
      styles/
    src-tauri/
      src/
        shell/              window, shortcut, lifecycle
        commands/           narrow Tauri command handlers
        application/        use cases and approval orchestration
        connectors/         Gmail, Calendar, fixture adapters
        domain/             open-loop state and priority rules
        inference/          extraction, evidence checks, briefing
        persistence/        SQLx repositories
        security/           encryption, redaction, Keychain adapter
        jobs/               persisted worker and retry policy
        lib.rs
      migrations/
      capabilities/
        main.json
      tauri.conf.json
    tests/
      frontend/
      e2e/
      evals/
      fixtures/
    src-tauri/tests/
      unit/
      integration/
      contract/
    scripts/
      package/
      release/

This is one application, not a monorepo. Module boundaries are enforced through import rules and tests rather than separate deployment units.

## 10. Domain model

### 10.1 Core concepts

**Source event:** An append-only normalized observation of an email, message, calendar mutation, or user command. An edited provider item creates a new local revision rather than changing evidence already used by a loop.

**Open loop:** A durable belief that an obligation remains unresolved.

**Evidence:** One or more exact excerpts and source identifiers that support the belief.

**Owner:** Who currently owes the next action.

**Status:** Where the loop is in its lifecycle.

**Action:** A proposed or completed mutation, including calendar creation.

**Transition:** An append-only record explaining how and why domain state changed.

### 10.2 Ownership

Ownership is one of:

- me;
- other;
- shared;
- unknown.

The value other requires at least one linked person identity. Unknown items enter review rather than being presented as certain.

### 10.3 Lifecycle

Status is one of:

- needs_review;
- open;
- scheduled;
- resolution_suggested;
- resolved;
- dismissed;
- superseded.

The state machine is:

    extracted candidate
           |
           +--> needs_review --user confirms--> open
           |
           +--> open
                 |
                 +--> scheduled
                 |       |
                 |       +--> resolution_suggested
                 |
                 +--> resolution_suggested --user or strong rule--> resolved
                 |
                 +--> dismissed
                 |
                 +--> superseded

    resolved --new contradictory evidence--> open

A model can suggest a transition. Deterministic policy or explicit user intent applies it. A user correction writes a transition and becomes a constraint for later reconciliation.

## 11. SQLite schema

All identifiers use UUIDv7 values generated locally. Timestamps are stored in UTC milliseconds. Original provider timezone and offset are retained when present.

### 11.1 Tables

**connectors**

- id
- kind: gmail, google_calendar, fixture_messages
- account_label
- status: connected, syncing, stale, error, disconnected
- granted_scopes
- last_synced_at
- last_error_code
- created_at, updated_at

Secrets are not stored in this table.

**sync_cursors**

- connector_id
- stream
- cursor
- cursor_created_at
- last_full_sync_at
- updated_at

Unique key: connector_id + stream.

**people**

- id
- display_name_ciphertext
- relationship_weight
- created_at, updated_at

**identity_aliases**

- id
- person_id
- connector_id
- provider_identity_hash
- address_ciphertext

Unique key: connector_id + provider_identity_hash.

**source_threads**

- id
- connector_id
- provider_thread_id
- subject_ciphertext
- participant_hashes
- latest_event_at

Unique key: connector_id + provider_thread_id.

**source_events**

- id
- connector_id
- thread_id
- provider_event_id
- provider_revision
- previous_event_id, nullable
- is_current
- event_type
- direction: incoming, outgoing, system
- occurred_at
- timezone
- sender_identity_hash
- recipient_identity_hashes
- content_ciphertext
- content_hash
- metadata_ciphertext
- ingested_at

Unique key: connector_id + provider_event_id + provider_revision.

Indexes: thread_id + occurred_at; connector_id + occurred_at; connector_id + provider_event_id + is_current; content_hash.

**open_loops**

- id
- title_ciphertext
- owner
- owner_person_id, nullable
- status
- origin: inferred, user
- due_at, nullable
- due_timezone, nullable
- waiting_since, nullable
- confidence from 0 to 1
- priority_score from 0 to 100
- pinned
- user_constraint_ciphertext, nullable
- scheduled_calendar_block_id, nullable
- version
- created_at, updated_at, resolved_at

Indexes: status + priority_score; owner + status; due_at.

**open_loop_evidence**

- id
- open_loop_id
- source_event_id
- quote_ciphertext
- start_offset
- end_offset
- evidence_kind: request, promise, deadline, follow_up, completion, cancellation
- created_at

Unique key: open_loop_id + source_event_id + start_offset + end_offset.

**loop_transitions**

- id
- open_loop_id
- from_status
- to_status
- actor: user, rule, model_suggestion, provider
- reason_ciphertext
- evidence_id, nullable
- model_run_id, nullable
- created_at

This table is append-only.

**calendar_blocks**

- id
- connector_id, nullable for local drafts
- provider_event_id, nullable
- title_ciphertext
- category: meeting, execution, rest, reminder, other
- starts_at
- ends_at
- timezone
- provider_revision, nullable
- sync_status: local, pending, synced, conflict, failed
- version
- created_at, updated_at

Unique key when present: connector_id + provider_event_id.

**actions**

- id
- kind: create_calendar, update_calendar, create_task, correct_loop
- target_id, nullable
- payload_ciphertext
- preview_ciphertext
- status: draft, awaiting_approval, approved, executing, succeeded, failed, cancelled
- idempotency_key
- expected_version, nullable
- provider_result_ciphertext, nullable
- error_code, nullable
- created_at, approved_at, completed_at

Unique key: idempotency_key.

**jobs**

- id
- kind
- payload_ciphertext
- idempotency_key
- status: pending, running, retry, completed, dead
- attempts
- available_at
- lease_expires_at, nullable
- last_error_code, nullable
- created_at, updated_at

Unique key: kind + idempotency_key.

Dead is the local dead letter queue state: storage for work that exhausted automatic retries and needs inspection or a user-triggered retry.

**model_runs**

- id
- purpose: extract, reconcile, command_parse, briefing
- provider
- model
- prompt_version
- input_hash
- output_hash, nullable
- schema_valid
- evidence_valid
- latency_ms
- input_tokens, output_tokens
- outcome
- error_code, nullable
- created_at

Raw model inputs and outputs are not stored in this table.

**settings**

- key
- value_ciphertext
- updated_at

### 11.2 Encryption at rest

V1 uses application-level envelope encryption:

1. generate a random 256-bit local data key;
2. store that key as an application-scoped generic-password item in macOS Keychain through a Rust SecretStore adapter;
3. encrypt sensitive columns with AES-256-GCM and a unique nonce per value;
4. store only non-sensitive identifiers, hashes, status values, and timestamps in plaintext.

OAuth refresh tokens and model API keys are separate Keychain items and are never exposed to the webview.

The database enables secure_delete. Connector deletion also checkpoints and truncates the write-ahead log before a maintenance vacuum so removed plaintext cannot remain in normal free pages. Backups contain the same encrypted sensitive columns.

### 11.3 Concurrency

SQLite runs in write-ahead logging mode. A write-ahead log is an append-only change file that lets readers continue while one writer commits. Application writes still pass through one repository boundary and short transactions.

Optimistic concurrency uses the version field. Every user mutation includes expectedVersion. A stale request returns a conflict payload with the current object rather than silently overwriting it.

Pre-migration snapshots use SQLite VACUUM INTO after the job worker reaches a quiescent point rather than copying only the main database file. This produces a consistent recovery database that includes committed write-ahead-log content.

## 12. Connector architecture

Every connector implements:

    connect()
    disconnect()
    fullSync(window)
    incrementalSync(cursor)
    normalize(rawItem)
    health()

Only Calendar connectors additionally implement previewWrite() and executeApprovedWrite().

### 12.1 Gmail

V1 requests the narrowest read-only scope that supports message synchronization.

Initial sync:

- fetch the latest 30 days by default;
- retrieve headers and plain-text bodies for selected messages;
- cap initial ingestion at 1,000 messages;
- let the user expand the time window explicitly.

Incremental sync:

- store Gmail historyId after a successful full sync;
- request changes through users.history.list;
- process additions, deletions, and label changes;
- commit normalized events and the new cursor in one local transaction.

If Gmail returns 404 because the history cursor expired, Kyra marks the connector stale and performs a new bounded full sync. It does not wipe the derived open-loop history first. Reconciliation determines whether provider deletions affect evidence.

### 12.2 Google Calendar

Initial sync:

- fetch events from 30 days in the past through 90 days in the future;
- retain cancelled events long enough to reconcile local blocks;
- preserve event IDs, revisions, recurrence identifiers, attendee state, and timezone.

Incremental sync uses Google's syncToken. A 410 response means the token is invalid and triggers a rebuild of the connector's event cache, followed by reconciliation against local blocks.

Calendar writes use a two-step operation:

1. create a local action with normalized title, time, timezone, category, and collision warnings;
2. show the preview and require confirmation;
3. execute using a stable idempotency key stored in the provider event's private extended properties;
4. reconcile the provider response into calendar_blocks.

An ambiguous network timeout is not blindly retried as a fresh create. Kyra first queries the provider for the private Kyra action ID to determine whether the event already exists.

### 12.3 Fixture message connector

The demo connector reads versioned JSON fixtures with:

- thread and message IDs;
- participant identities;
- direction;
- timestamps and timezone;
- message body;
- edited or deleted state.

It supports incremental fixture batches so a second batch can close or contradict loops created by the first. This proves reconciliation without pretending to have live WhatsApp access.

### 12.4 Sync schedule

- manual sync at any time;
- automatic sync every three minutes while online and awake;
- a five-second debounce after wake or network restoration;
- exponential retry with random jitter and provider Retry-After support;
- maximum connector concurrency of two;
- visible last-successful-sync time in the interface.

Rate limiting means a provider temporarily refuses requests after a usage threshold. Kyra treats this as a recoverable state and preserves the pending job.

## 13. OAuth and secrets

Google uses the installed-application authorization flow:

- open the user's system browser;
- use PKCE, a one-time verifier that protects an intercepted authorization code;
- receive the redirect on a random 127.0.0.1 loopback port;
- exchange the code in the Rust core;
- store the refresh token through the Keychain SecretStore adapter.

The desktop application is a public OAuth client and cannot keep a client secret confidential. PKCE and exact redirect validation are therefore mandatory.

Disconnecting a connector:

1. shows what local data and derived loops depend on it;
2. revokes the token when possible;
3. deletes the local secret;
4. lets the user retain derived loops without raw source text or delete both;
5. records the choice in the local audit log.

Google classifies Gmail scopes as sensitive or restricted depending on scope. Test users can run during development, but public distribution requires OAuth verification and may require an external security assessment when restricted data passes through third-party infrastructure. This is a launch gate, not a detail to discover after implementation.

## 14. Inference pipeline

### 14.1 Model boundary

The model is a structured proposal engine. It cannot:

- call application tools;
- access provider tokens;
- issue network requests;
- write to SQLite;
- change an open-loop state;
- execute calendar or message actions.

Email and message bodies are untrusted data. Prompt injection means text inside a source attempts to override the application's instructions. Kyra defends against it by passing source content only as delimited data, giving the model no tools, validating every output, and requiring exact evidence.

### 14.2 Extraction stages

    normalized source events
            |
            v
    deterministic thread grouping
            |
            v
    local redaction and pseudonymization
            |
            v
    model returns structured candidates
            |
            v
    schema validation
            |
            v
    exact quote and event-ID validation
            |
            v
    deterministic identity and loop matching
            |
            v
    create, update, or flag for review

The model output contract contains:

- a short action-oriented title;
- owner and counterparty tokens;
- proposed due time and source timezone;
- candidate lifecycle hint;
- confidence;
- one or more evidence entries containing event ID, exact quote, and evidence kind;
- an ambiguity list;
- a possible relation to an existing loop ID.

The validator rejects a candidate if:

- its schema is invalid;
- an evidence event is not in the supplied batch;
- the quoted excerpt does not exactly occur in the event;
- ownership is asserted without supporting context;
- the output contains instructions rather than facts;
- a due date cannot be resolved to a timezone.

One repair attempt is allowed for invalid JSON. A second failure records a model-run error and leaves the batch available for retry or deterministic review.

### 14.3 Local redaction and pseudonymization

Before a cloud call:

- known names, email addresses, phone numbers, and account identifiers become stable local tokens;
- URLs have query strings removed unless required as evidence;
- attachment bytes are excluded;
- signatures and quoted reply history are trimmed deterministically;
- the user can inspect a developer-mode outbound preview.

The token-to-person mapping never leaves the application. Pattern redaction cannot guarantee removal of every possible identifier, so V1 must describe this honestly and test it. A future local model can replace cloud extraction through the same adapter.

### 14.4 Deduplication

Deduplication uses deterministic features before model judgment:

- same source thread;
- overlapping participant identities;
- normalized title tokens;
- evidence event overlap;
- due-time proximity;
- existing unresolved loop window;
- explicit provider references.

A model may propose same_as_existing_loop_id, but code applies the merge only above a configured score or asks the user when confidence is lower. Merges are reversible because evidence and transitions remain append-only.

### 14.5 Reconciliation

New events are evaluated against active loops linked to the same thread or people. The reconciliation result can propose:

- fulfilled;
- cancelled;
- superseded;
- deadline changed;
- owner changed;
- still open;
- ambiguous.

V1 automatically applies only high-confidence, deterministic cases such as a direct reply containing the promised artifact after an explicit promise. Other cases become resolution_suggested. The interface shows the new evidence and lets the user confirm or reject it.

## 15. Priority and the Night briefing

Priority is transparent and deterministic. The model does not secretly rank the task list.

The initial score is capped at 100:

| Signal | Points |
|---|---:|
| User pin | Immediate 100 |
| Explicit deadline urgency or overdue state | 0 to 35 |
| User owns the next action | 0 to 20 |
| Explicit promise or request | 0 to 15 |
| Relationship importance set locally | 0 to 10 |
| Staleness and repeated follow-up | 0 to 10 |
| Extraction confidence | 0 to 5 |
| Scheduled time is approaching | 0 to 5 |

These weights are a starting hypothesis and live in versioned configuration. The UI exposes a plain-language reason such as "due today, you promised this, and RC followed up."

Night selects at most three active loops, balancing:

- at least one user-owned item when present;
- genuinely urgent waiting-on-other items;
- calendar feasibility;
- no duplicate loops from the same commitment.

The model may verbalize the selected structured facts into one or two sentences. A deterministic template is the fallback. The briefing never introduces a person, deadline, or action absent from the selected loop records.

## 16. Command and calendar behavior

### 16.1 /task

Input:

    /task send RC the pitch update tomorrow

Flow:

1. deterministic slash-command dispatch;
2. deterministic date parsing first;
3. model fallback only for ambiguous natural language;
4. preview when ownership or date is ambiguous;
5. insert a user-origin open loop;
6. write an audit transition;
7. show Added by you.

### 16.2 /cal

Input:

    /cal deep work on Kyra tomorrow 9am for 90 minutes

Flow:

1. parse title, local date, start, duration, timezone, and category;
2. show conflicts and the proposed block;
3. require confirmation;
4. create a local action and provider event;
5. update the planner from the reconciled provider result.

### 16.3 Planner interactions

Move, resize, recolor, and grid-selection all create local action drafts. Existing provider events require confirmation before the write. Local-only execution blocks can be created immediately, with a visible local status until a user chooses to sync them.

Drag callbacks include the block version captured at drag start. If connector sync changes the event during the interaction, the write is rejected as a race condition, meaning two operations attempted to change the same state concurrently. The user sees the current event and can retry.

## 17. Persisted job system

Jobs survive app restarts. The worker claims a row by setting status to running and a short lease_expires_at value inside a transaction.

Rules:

- one writer claims a job;
- expired leases return to retry;
- connector requests retry up to five times;
- model requests retry once for transport failure and once for invalid structure;
- permanent authorization errors do not retry;
- each job has an idempotency key;
- failed jobs remain inspectable and can be retried manually;
- network restoration wakes eligible jobs.

Job classes:

- gmail_full_sync;
- gmail_incremental_sync;
- calendar_full_sync;
- calendar_incremental_sync;
- normalize_batch;
- extract_commitments;
- reconcile_loops;
- recompute_priorities;
- generate_briefing;
- execute_calendar_action;
- cleanup_retention.

Dependency chaining is data-driven. For example, a sync transaction enqueues normalize_batch for the new event IDs; successful normalization enqueues extraction. The UI never waits synchronously for a model call.

## 18. Security and privacy

### 18.1 Desktop hardening

- only the bundled main webview receives a Tauri capability;
- the Rust application manifest and generated permissions allow only the named custom commands required by that webview;
- SQL, HTTP, shell, filesystem, process, and secret-store plugin permissions are not exposed to frontend JavaScript;
- navigation and new-window creation denied by default;
- strict Content Security Policy with no unsafe-eval;
- Tauri's default custom protocol rather than a localhost production server;
- no remote web content receives Tauri capabilities;
- Serde deserialization plus domain validation in every Rust command;
- Zod validation before frontend invocation and after responses;
- external URLs opened only after allowlist validation;
- unsafe Rust forbidden in Kyra application crates;
- Cargo.lock and pnpm-lock.yaml committed and dependency versions audited.

### 18.2 Data minimization

- Default connector windows are bounded.
- Raw source bodies expire after 90 days unless they support an active loop.
- Resolved-loop raw evidence expires after a configurable 30-day grace period.
- Open-loop title, reasoning, source excerpts, addresses, and action previews are encrypted.
- Logs contain IDs, counts, durations, and error codes, not message bodies.
- Model-run records store hashes and metrics, not raw prompts.
- Diagnostic export previews its exact contents before writing.

### 18.3 Approval boundary

V1 autonomy levels are:

1. **Observe:** synchronize permitted data.
2. **Infer:** propose an evidence-backed open loop.
3. **Organize:** prioritize, brief, and create local tasks.
4. **Draft:** prepare a calendar mutation.
5. **Execute after approval:** write the confirmed calendar action.

Sending messages and other irreversible external actions are absent from V1.

### 18.4 Threats and controls

| Threat | Control |
|---|---|
| Malicious content tries to command the model | No model tools, source delimiters, exact evidence validation |
| Webview compromise seeks tokens | Secrets and network clients exist only in the Rust core and no secret capability is granted |
| Duplicate sync creates duplicate loops | Provider unique keys, content hashes, idempotent upserts |
| Stolen database reveals conversations | Sensitive columns encrypted with a Keychain-protected data key |
| Stale UI overwrites newer provider state | Object versions and expectedVersion checks |
| Model invents evidence | Exact substring and source-ID verification |
| User approves a different action than previewed | Approval signs the immutable action ID and payload hash |
| OAuth redirect is intercepted | PKCE, loopback-only redirect, state and exact redirect validation |

## 19. Failure handling

| Failure | Expected behavior |
|---|---|
| Command+K is already owned by another app | Registration failure is detected; menu-bar and configurable shortcut fallback are shown |
| App is offline | Last synchronized dashboard remains usable; pending work is queued and labeled stale |
| Google token is revoked | Connector enters reconnect-required state; local data remains readable |
| Gmail history cursor expires | Mark stale, run bounded full sync, then reconcile |
| Calendar sync token expires | Rebuild provider event cache, preserve local audit history, then reconcile |
| Provider rate limit | Respect Retry-After, back off with jitter, keep job pending |
| Model unavailable | Retain source events, show extraction pending, use template briefing from existing loops |
| Model returns invalid or unsupported output | Reject it, retry once, record the error, never mutate loop state |
| App crashes during a write | SQLite transaction rolls back; expired job lease makes work retryable |
| App crashes after a provider write but before local commit | Reconcile by stable action marker before any retry |
| Database migration fails | Restore the pre-migration snapshot and open in read-only recovery mode |
| Disk is full | Stop new sync and model work, preserve reads, show a blocking storage banner |
| System timezone changes | Store UTC plus source timezone; recalculate display without rewriting historical instants |
| User deletes a connector | Preview dependent data, delete secrets first, apply chosen retention policy, then compact storage |

## 20. Performance budgets

Budgets are measured on an Apple-silicon Mac with 10,000 source events, 1,000 loops, and 2,000 calendar blocks.

| Operation | V1 budget |
|---|---:|
| Warm global shortcut to visible overlay, p95 | under 150 ms |
| Cold launch to usable cached dashboard, p95 | under 2.5 s |
| Dashboard query, p95 | under 250 ms |
| Loop correction persisted and reflected | under 100 ms |
| Calendar drag feedback | 60 frames per second |
| Incremental local reconciliation after provider data arrives, p95 | under 10 s excluding provider and model latency |
| First useful view during initial Gmail sync | under 15 s |
| Idle app memory | under 150 MB |
| Signed application bundle before DMG compression | under 35 MB |
| Local database at default retention | under 500 MB for the reference dataset |

Controls:

- virtualize the open-loop list;
- load encrypted source bodies only for visible details;
- query dashboard projections, not full source events;
- batch at most 20 source threads per model request;
- run at most two model requests concurrently;
- debounce planner writes;
- precompute priority and dashboard projections after transitions;
- capture Rust core, webview, and query timings in local sanitized metrics.

No cache is introduced until profiling shows a repeated expensive read. SQLite projections should be sufficient for V1.

## 21. Observability

V1 has local, privacy-preserving observability:

- JSON logs with timestamp, subsystem, operation ID, connector ID, duration, outcome, and error code;
- no raw message bodies, names, addresses, or model prompts;
- model metrics for latency, token counts, schema validity, and evidence validity;
- connector health with last attempt and last successful sync;
- job queue counts by status;
- transition history on every loop;
- a user-visible diagnostics page;
- an export bundle that is redacted and previewed.

The demo build includes a local health panel behind a developer shortcut. It shows connectors, queue depth, model adapter, database migration version, and last reconciliation result.

## 22. Test and evaluation architecture

High precision matters more than aggressive recall. A system that repeatedly invents obligations loses trust.

### 22.1 Test layers

**Unit tests**

- state-transition rules;
- priority scoring;
- time and timezone parsing;
- ownership constraints;
- deduplication features;
- redaction and pseudonymization;
- evidence quote validation;
- action approval hashes;
- retry classification;
- every security-sensitive branch.

Target: 100 percent branch coverage for domain, security, Tauri command validation, and action policy modules.

**SQLite integration tests**

- migrations forward from every released schema;
- rollback and snapshot recovery;
- concurrent reads and serialized writes;
- unique-key idempotency;
- job lease expiry;
- secure connector deletion;
- version conflicts;
- crash boundaries around action state.

**Connector contract tests**

- sanitized Gmail history fixtures;
- history cursor expiration;
- Calendar syncToken invalidation;
- recurring and cancelled events;
- rate limits and Retry-After;
- OAuth reconnect states;
- provider write timeout reconciliation.

**Model evaluations**

At least 60 versioned cases:

- direct request;
- direct promise;
- delegated work;
- follow-up;
- rejection;
- hypothetical language;
- past or already completed work;
- changed deadline;
- changed owner;
- duplicate messages;
- ambiguous ownership;
- multiple people with similar names;
- timezone and relative-date cases;
- completion evidence;
- cancellation and supersession;
- quoted old messages;
- source-level prompt injection;
- unsupported model evidence.

Initial quality gates:

- schema validity: 100 percent;
- evidence support: 100 percent;
- precision on active-loop creation: at least 95 percent;
- recall on explicit requests and promises: at least 80 percent;
- unsupported automatic resolution: 0 cases;
- deterministic fake-provider suite: 100 percent repeatable.

**Desktop end-to-end tests**

- first-run onboarding with fake providers;
- Command+K overlay activation through a shell adapter;
- Escape and click-away dismissal;
- today and three-day planner navigation;
- /task creation and Added by you provenance;
- /cal preview, approval, and synchronized result;
- drag, resize, recolor, and conflict handling;
- evidence detail opening;
- user correction;
- fixture batch closes an existing loop;
- offline and stale states;
- connector reconnect;
- packaged DMG smoke launch.

WebdriverIO uses @wdio/tauri-service with its embedded macOS driver for packaged end-to-end tests. Backend command mocking and log access are enabled only in the test build; the WebdriverIO plugins are excluded from release builds. Native dialogs and global OS shortcuts remain isolated behind adapters, with a small manual release checklist for behavior automation cannot reliably control.

### 22.2 Critical-path coverage

    App launch
      +-> migration + encrypted key load [UNIT + INTEGRATION]
      +-> cached dashboard query [INTEGRATION]
      +-> overlay render [E2E]

    Connector sync
      +-> OAuth token retrieval [CONTRACT]
      +-> incremental provider fetch [CONTRACT]
      +-> normalized idempotent upsert [INTEGRATION]
      +-> extraction job [EVAL]
      +-> evidence verification [UNIT + EVAL]
      +-> loop transition [UNIT + INTEGRATION]
      +-> Night projection [UNIT + E2E]

    /task
      +-> command dispatch [UNIT]
      +-> date parsing [UNIT + EVAL]
      +-> local loop insert [INTEGRATION]
      +-> provenance visible [E2E]

    /cal
      +-> parse + timezone [UNIT + EVAL]
      +-> collision preview [UNIT + E2E]
      +-> explicit approval [UNIT + E2E]
      +-> idempotent provider write [CONTRACT]
      +-> reconciled calendar block [INTEGRATION + E2E]

    Later completion evidence
      +-> incremental fixture batch [CONTRACT]
      +-> reconcile proposal [EVAL]
      +-> deterministic safe transition [UNIT]
      +-> resolved state and audit [INTEGRATION + E2E]

## 23. Distribution and updates

### 23.1 Build artifacts

- arm64 and x64 macOS builds;
- signed DMG for installation;
- signed .app.tar.gz updater artifacts and detached .sig files;
- generated checksums;
- software bill of materials;
- release notes tied to the Git commit.

### 23.2 Signing and notarization

Release builds run on a controlled CI runner with:

- Apple Developer ID Application certificate;
- hardened runtime;
- entitlements limited to required capabilities;
- notarization through Apple's service;
- stapled notarization ticket;
- Gatekeeper smoke test on a clean macOS account.

Developer builds remain visually distinct and never reuse production OAuth credentials.

### 23.3 Updates

The first external demo can use manual signed releases. Automatic updates are enabled only after:

- signing and notarization are stable;
- the update feed is HTTPS-only;
- the Tauri updater public key is embedded and artifact signature verification passes;
- update failures preserve the working version;
- rollback has been rehearsed.

A feature flag, meaning a local configuration switch that can disable a capability without shipping new code, controls connector rollout and model-provider selection. A local kill switch, meaning an emergency switch that stops an unsafe operation, can disable all external writes while keeping the application readable.

## 24. Delivery plan

### Phase 0: Foundation

- Tauri 2, Rust, Tokio, React, Vite, TypeScript, pnpm, and Cargo.
- Transparent Tauri window, global shortcut plugin, main-window capability, and typed command boundary.
- SQLx SQLite migrations, Keychain SecretStore, encryption key, repositories, and seed data.
- Deterministic clocks, IDs, and provider fakes.
- CI for cargo fmt, Clippy, Rust tests, frontend lint, typecheck, Vitest, and package smoke.

Exit: signed local development app opens with Command+K and renders seeded data.

### Phase 1: Open-loop engine

- Source-event schema and fixture connector.
- Structured extraction adapter and deterministic fake.
- Evidence validation.
- Ownership, lifecycle, deduplication, reconciliation, and transitions.
- Priority score and template briefing.
- First 60-case model evaluation set.

Exit: two fixture batches create, update, and resolve evidence-backed loops with passing quality gates.

### Phase 2: Product surface

- Ambient overlay and dismissal behavior.
- Today timeline and three-day planner.
- Open-loop list and evidence details.
- Night briefing.
- /task and /cal parsing.
- User correction and status controls.

Exit: complete fixture-backed demo acceptance flow works without manual database changes.

### Phase 3: Real connectors

- Google installed-app OAuth.
- Gmail bounded full and incremental sync.
- Calendar full and incremental sync.
- Calendar preview and approved writes.
- Reconnect, rate-limit, cursor-expiration, and offline states.

Exit: a test Google account completes the same demo flow with real Calendar and Gmail data.

### Phase 4: Trust and recovery

- Pseudonymization and outbound model preview.
- Retention controls and connector deletion.
- Encrypted diagnostics.
- Job recovery and provider-write reconciliation.
- Performance profiling and budgets.
- Security review against Tauri capabilities, command scopes, and updater guidance.

Exit: all failure-mode tests pass and the app remains useful offline.

### Phase 5: Release proof

- macOS signing and notarization.
- DMG plus signed Tauri updater artifact generation.
- Clean-machine installation.
- End-to-end demo recording.
- Exact setup and privacy documentation.

Exit: a new user can install the signed build, connect a test account, and reproduce the acceptance flow.

## 25. Work sequencing

The first shared milestone is non-parallel: contracts, schema, migration harness, encryption boundary, test fixtures, and Tauri command conventions must be locked together.

After that, three lanes can proceed independently:

    Shared contracts and database foundation
                    |
          +---------+---------+
          |                   |
          v                   v
    Lane A: shell/UI     Lane B: domain/inference
          |                   |
          +---------+---------+
                    |
                    v
             fixture-backed E2E
                    |
                    v
          Lane C: Google connectors
                    |
                    v
          trust, packaging, release

Lane C can start OAuth registration early, but real connector integration should target the proven domain contracts rather than inventing a second model.

## 26. Acceptance criteria

V1 is complete only when a packaged build can demonstrate all of the following:

1. Open through Command+K and dismiss with Escape or click-away.
2. Show a current-time timeline and an editable three-day planner.
3. Ingest at least five representative commitments.
4. Distinguish user-owned work, waiting-on-other work, shared work, and ambiguous work.
5. Show exact source evidence for every inferred loop.
6. Select a meaningful top-three Night briefing rather than restating the full list.
7. Create one /task with Added by you provenance.
8. Preview and approve one /cal event.
9. Move, resize, create, and recolor calendar blocks.
10. Correct one inferred owner or deadline and preserve the correction.
11. Apply a later fixture batch that resolves one existing loop.
12. Recover from an expired provider cursor without duplicating loops.
13. Remain readable offline and clearly label stale data.
14. Send no external message and perform no calendar write without approval.
15. Pass the model quality, security, migration, performance, and packaged-app gates.

## 27. Major risks and launch gates

| Risk | V1 response | Gate |
|---|---|---|
| False-positive loops destroy trust | Exact evidence, high precision gate, needs-review state | 95 percent precision and 100 percent evidence support |
| Gmail access blocks public launch | Start with test accounts and narrow scopes | Google OAuth verification plan approved before public beta |
| Personal WhatsApp has no safe connector | Use clearly labeled fixtures | Never claim live WhatsApp support |
| Cloud model sees sensitive text | Local minimization, encryption, pseudonymization, direct BYOK | Redaction suite and user-visible privacy description |
| Webview compromise reaches privileged commands | Minimal Tauri capabilities, narrow Rust commands, no direct SQL or HTTP permissions | Capability audit and command-boundary tests |
| Calendar write duplicates events | Stable action marker and reconciliation | Timeout contract test passes |
| Relative time is interpreted incorrectly | Deterministic parsing, retained timezone, preview | Timezone test matrix passes |
| Model behavior drifts | Version prompts, provider adapter, golden evaluations | Evals run for every model or prompt change |
| Local database becomes corrupted | Transactions, snapshots, recovery mode | Migration and crash recovery drills pass |
| Visual clone hides weak product behavior | Acceptance requires full ingest-to-reconcile flow | No static-only demo accepted |

## 28. Decisions intentionally deferred

- Whether a local model is the default after V1.
- Whether the overlay eventually needs a small native AppKit extension beyond Tauri's window APIs.
- Whether to add a hosted synchronization service.
- Whether to support Windows.
- Whether any outbound message action is safe enough for a later version.
- Which official or user-mediated message connector replaces fixtures.
- How priority weights should learn from long-term user behavior.

These decisions do not block the V1 architecture. Each has a stable adapter or data boundary where a later implementation can attach.

## 29. Official technical references

- [What is Tauri?](https://tauri.app/start/)
- [Tauri configuration and window options](https://v2.tauri.app/reference/config/)
- [Tauri global shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/)
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri permissions](https://v2.tauri.app/security/permissions/)
- [Tauri official plugins](https://v2.tauri.app/plugin/)
- [Tauri WebdriverIO testing](https://v2.tauri.app/develop/tests/webdriver/)
- [Tauri distribution](https://v2.tauri.app/distribute/)
- [Tauri DMG distribution](https://v2.tauri.app/distribute/dmg/)
- [Tauri macOS signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Gmail synchronization](https://developers.google.com/workspace/gmail/api/guides/sync)
- [Gmail history API](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.history/list)
- [Google Calendar synchronization](https://developers.google.com/workspace/calendar/api/guides/sync)
- [Google OAuth for native apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google OAuth best practices](https://developers.google.com/identity/protocols/oauth2/resources/best-practices)
- [Google restricted-scope verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)
- [SQLite write-ahead logging](https://sqlite.org/wal.html)
- [SQLite isolation](https://www.sqlite.org/isolation.html)
- [FullCalendar documentation](https://fullcalendar.io/docs)
- [FullCalendar drag and resize](https://fullcalendar.io/docs/event-dragging-resizing)
- [Vitest coverage](https://main.vitest.dev/guide/coverage)

## 30. Final architectural position

Kyra V1 should be a local desktop commitment engine with a beautiful ambient surface, not a calendar clone with an AI textbox.

The application succeeds when it can explain:

- what the loop is;
- who owns the next action;
- which source evidence created that belief;
- why it matters now;
- what Kyra proposes to do;
- what actually happened afterward.

If those answers are accurate, inspectable, and continuously reconciled, the V1 demonstrates the product thesis. Everything else can grow from that foundation.
