# Kyra AI Engine V1

**Status:** Implemented native V1 foundation

**Date:** 18 August 2026

## Boundary

React renders setup, status, Command+K, reviews, and activity. Rust owns provider credentials, network calls, decryption, inference contracts, policy, SQLite mutations, Google Calendar writes, jobs, audit, and recovery. Models receive no tools and can only return a versioned `IntentEnvelope`.

One provider is selected at a time:

- OpenAI Responses API with strict structured output and `store: false`;
- Anthropic Messages with structured output;
- Ollama structured output through a loopback-only URL;
- a deterministic fake provider used by activation and tests.

There is no provider fallback. API keys are separate macOS Keychain entries and never cross the Tauri response boundary.

## Secure local model

An application AES-256-GCM key protects local loop payloads, evidence, transitions, reviews, command sessions, model-derived records, and action snapshots. Google source records retain their per-account data key. Every value uses a fresh nonce.

Startup migrates legacy plaintext records transactionally and verifies decryption before blanking plaintext. AI work stays blocked until this migration completes. A missing application key is not replaced automatically.

Gmail synchronization stores immutable `provider_item_revisions`. A current provider item points to its latest revision; changed or removed items create a new revision or tombstone. Inference records the exact revision, canonical document hash, byte offsets, and quote hash. Rust checks all of them against the transformed document before accepting a proposal.

Cloud prompts replace known aliases with stable HMAC-derived person IDs. The canonicalizer removes common signatures and duplicated quoted history, caps a thread at 24,000 UTF-8 characters, preserves the first and newest messages, and marks truncation. Truncated input cannot cause a passive Calendar write.

## Persistent runtime

Connector ingestion transactionally produces source revisions and idempotent extraction jobs. Jobs bind account generation, source generation, and the active model activation fingerprint. Workers use random claim tokens, lease heartbeats, compare-and-set completion, bounded retry with jitter, dead-letter state, and concurrency limits of two cloud requests or one Ollama request.

Reconciliation waits for a completed source-generation barrier. A source sweeper recreates missing jobs from immutable revisions. Changing a provider, model, credential generation, source generation, or Google account fences stale results before commit. Disconnect increments the Google account generation, cancels queued work, allows leased work up to five seconds to drain, then purges the account graph and Keychain records.

## Activation and evaluation

A provider must pass 12 representative cases before it becomes ready. The activation fingerprint binds provider, requested and resolved model identity, Ollama digest where available, credential/config generations, prompt/schema/policy/redaction versions, application version, and activation time.

The gate requires:

- 100% schema validity and valid evidence references;
- zero unauthorized actions;
- zero autonomous actions for ambiguous Calendar cases;
- at least 90% required-action coverage;
- at least 80% confirmed-meeting recall;
- no call over 90 seconds.

Cloud activation expires after seven days. The checked-in `kyra-eval-v1` corpus contains more than 80 provider-neutral request, promise, delegation, ambiguity, identity, time-zone, meeting, injection, malformed-input, truncation, and command cases. CI validates its coverage and uses deterministic providers and HTTP mocks; live evaluation is opt-in.

## Deterministic policy

Rust validates schema versions, activation fingerprints, source hashes, record identities, person IDs, exact evidence, time ranges, time zones, attendees, recurrence, event versions, and supported actions. Confidence is diagnostic only.

Passive Gmail inference may create or update evidence-backed open loops. It never deletes or silently resolves a loop. Ambiguity, truncation, cross-thread duplicates, changed sources, or conflicts with user-authored derivations enter review.

A passive Calendar create or reschedule requires two distinct participants' matching proposal and acceptance, explicit start/end/date/time-zone data, resolved attendees, no duplicate event, no newer provider version, and—when rescheduling—an event Kyra created. Passive writes use `sendUpdates: none`; destructive changes and attendee edits enter review.

Natural-language Command+K actions may mutate tasks and Calendar records. Missing details create one encrypted, ten-minute, one-use clarification session with a fixed time anchor. Attendee notifications produce a confirmation listing recipient addresses. Stale Calendar versions stop and resynchronize instead of overwriting.

## Audit and recovery

Accepted actions store encrypted before/after snapshots, evidence/provenance, model-run identity, policy result, versions, and irreversible effects. Task Undo succeeds only when the task still has the action's resulting version. Calendar compensation likewise requires an unchanged event version:

- create compensation deletes the unchanged Kyra-created event;
- update compensation applies the previous supported snapshot;
- delete compensation recreates a supported snapshot as a new event.

The UI deliberately says **Compensate**, because invitations, cancellations, Meet links, organizer metadata, and recurrence side effects may not be reversible. Failed and dead-letter jobs appear in Activity with safe error text and manual Retry.

## Native interface

The existing Connections sheet configures Google and the active model, discovers Ollama models, shows activation/queue/review state, and keeps keys write-only. The Activity sheet presents evidence, proposed changes, irreversible effects, Accept/Dismiss, Retry, Undo, and Compensate. Tauri events invalidate status, dashboard, reviews, and actions without introducing another frontend state system.

The Night briefing remains constrained: Rust selects versioned facts, the model may only order their IDs and return enumerated role, urgency, subject, and action references, and Rust verifies those values against current routing state before rendering trusted sentence templates. It never displays unrestricted model prose; deterministic ordering and rendering are the fallback when provider generation is unavailable or invalid.
