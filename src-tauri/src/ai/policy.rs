use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    google::AiCalendarSnapshot,
    types::{
        CalendarEventInput, CalendarEventPatch, CalendarMutationInput, CalendarWhen,
        EvidencePayload, LoopPayload,
    },
};

use super::{
    runtime::{AiEngine, CompletedExtraction, EngineError},
    types::{AiActivityItem, AiReviewItem, IntentAction, IntentProposal},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PolicyMode {
    Passive,
    Reviewed,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LoopSnapshot {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub lifecycle: String,
    pub ownership: String,
    pub priority: i64,
    pub due_at: Option<String>,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReviewRecord {
    pub title: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub irreversible_effects: Vec<String>,
    pub proposal: IntentProposal,
    pub extraction: CompletedExtraction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActionRecord {
    pub title: String,
    pub detail: String,
    pub before_loop: Option<LoopSnapshot>,
    pub after_loop: Option<LoopSnapshot>,
    pub before_calendar: Option<AiCalendarSnapshot>,
    pub after_calendar: Option<AiCalendarSnapshot>,
    pub operation_id: Option<String>,
    pub irreversible_effects: Vec<String>,
}

impl AiEngine {
    pub(super) async fn apply_extraction(
        &self,
        extraction: &CompletedExtraction,
        mode: PolicyMode,
    ) -> Result<Vec<String>, EngineError> {
        let mut action_ids = Vec::new();
        for proposal in &extraction.envelope.proposals {
            let action = match proposal.action {
                IntentAction::TaskCreate => {
                    self.apply_task_create(extraction, proposal, mode).await?
                }
                IntentAction::TaskUpdate => {
                    self.apply_task_update(extraction, proposal, mode).await?
                }
                IntentAction::ResolutionSuggest => {
                    if mode != PolicyMode::Passive {
                        self.apply_task_resolution(extraction, proposal).await?
                    } else {
                        Some(
                            self.create_review(
                                "resolution_suggested",
                                "A loop may be complete",
                                "Kyra found later evidence of completion. Passive inference never resolves a loop silently.",
                                extraction,
                                proposal,
                                Vec::new(),
                            )
                            .await?,
                        )
                    }
                }
                IntentAction::CalendarCreate => {
                    self.apply_calendar_create(extraction, proposal, mode)
                        .await?
                }
                IntentAction::CalendarReschedule => {
                    self.apply_calendar_reschedule(extraction, proposal, mode)
                        .await?
                }
                IntentAction::CalendarCancel | IntentAction::CalendarDelete => {
                    if mode == PolicyMode::Passive {
                        Some(
                            self.create_review(
                                "calendar_destructive",
                                "Calendar change needs review",
                                "Passive inference cannot cancel or delete Calendar events.",
                                extraction,
                                proposal,
                                vec![
                                    "Attendee notifications and deleted Meet links may be irreversible."
                                        .to_owned(),
                                ],
                            )
                            .await?,
                        )
                    } else {
                        self.apply_calendar_delete(extraction, proposal).await?
                    }
                }
                IntentAction::BriefingOrder | IntentAction::NoAction => None,
            };
            if let Some(action) = action {
                action_ids.push(action);
            }
        }
        Ok(action_ids)
    }

    async fn apply_task_create(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        mode: PolicyMode,
    ) -> Result<Option<String>, EngineError> {
        if mode == PolicyMode::Passive
            && (extraction.truncated
                || proposal.ambiguity.is_some()
                || proposal.evidence.is_empty())
        {
            return Ok(Some(
                self.create_review(
                    "task_ambiguous",
                    "Possible open loop",
                    "The source was ambiguous or truncated, so Kyra did not create a loop automatically.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let title = required_title(proposal)?;
        let ownership = normalize_ownership(proposal.ownership.as_deref().unwrap_or("unknown"))?;
        validate_due_at(proposal.due_at.as_deref())?;
        let evidence_key = proposal
            .evidence
            .first()
            .map(|evidence| {
                format!(
                    "{}:{}:{}",
                    evidence.source_revision_id, evidence.start_offset, evidence.end_offset
                )
            })
            .unwrap_or_else(|| proposal.proposal_id.clone());
        let stable = self.cipher.pseudonymous_id(
            "ai-loop",
            &format!(
                "{}:{}:{}",
                extraction.account_id, extraction.thread_id, evidence_key
            ),
        );
        let loop_id = format!("ai_loop_{}", &stable[..24]);
        if let Some(existing) = self.load_loop_snapshot(&loop_id).await? {
            return self
                .update_ai_loop(extraction, proposal, existing, mode)
                .await;
        }
        let summary = proposal.summary.clone().unwrap_or_default();
        let payload = LoopPayload {
            title: title.clone(),
            summary: summary.clone(),
        };
        let (payload_nonce, payload_ciphertext) = self.cipher.encrypt(&payload)?;
        let now = Utc::now().to_rfc3339();
        let action_id = stable_action_id(
            &self.cipher,
            "task-create",
            &loop_id,
            &extraction.model_run_id,
        );
        let mut transaction = self.pool.begin().await?;
        let origin = if mode == PolicyMode::Command {
            "local"
        } else {
            "google"
        };
        sqlx::query("INSERT OR IGNORE INTO open_loops (id, title, summary, owner, status, priority, due_at, version, origin, lifecycle, ownership, payload_nonce, payload_ciphertext, payload_migrated, created_at, updated_at) VALUES (?, 'Encrypted', '', ?, ?, 70, ?, 1, ?, 'active', ?, ?, ?, 1, ?, ?)")
            .bind(&loop_id)
            .bind(legacy_owner(ownership))
            .bind(legacy_status(ownership))
            .bind(&proposal.due_at)
            .bind(origin)
            .bind(ownership)
            .bind(payload_nonce)
            .bind(payload_ciphertext)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
        self.persist_derivations(
            &mut transaction,
            &loop_id,
            extraction,
            proposal,
            &title,
            &summary,
            ownership,
            mode,
        )
        .await?;
        self.persist_evidence(&mut transaction, &loop_id, extraction, proposal, mode)
            .await?;
        let after = LoopSnapshot {
            id: loop_id.clone(),
            title: title.clone(),
            summary,
            lifecycle: "active".to_owned(),
            ownership: ownership.to_owned(),
            priority: 70,
            due_at: proposal.due_at.clone(),
            version: 1,
        };
        self.persist_action(
            &mut transaction,
            &action_id,
            "task_create",
            &extraction.account_id,
            &extraction.model_run_id,
            Some(&loop_id),
            None,
            Some("1"),
            ActionRecord {
                title: title.clone(),
                detail: "Created an evidence-backed open loop from Gmail.".to_owned(),
                before_loop: None,
                after_loop: Some(after),
                before_calendar: None,
                after_calendar: None,
                operation_id: None,
                irreversible_effects: Vec::new(),
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn apply_task_update(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        mode: PolicyMode,
    ) -> Result<Option<String>, EngineError> {
        let target = proposal.target_loop_id.as_deref().ok_or_else(|| {
            EngineError::Validation("A task update needs an exact loop target.".to_owned())
        })?;
        let existing = self.load_loop_snapshot(target).await?.ok_or_else(|| {
            EngineError::Validation("The target loop no longer exists.".to_owned())
        })?;
        self.update_ai_loop(extraction, proposal, existing, mode)
            .await
    }

    async fn update_ai_loop(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        before: LoopSnapshot,
        mode: PolicyMode,
    ) -> Result<Option<String>, EngineError> {
        let origin: String = sqlx::query_scalar("SELECT origin FROM open_loops WHERE id = ?")
            .bind(&before.id)
            .fetch_one(&self.pool)
            .await?;
        let has_user_derivation: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loop_derivations WHERE loop_id = ? AND source_type IN ('user', 'command') AND active = 1")
            .bind(&before.id)
            .fetch_one(&self.pool)
            .await?;
        if mode == PolicyMode::Passive && (origin != "google" || has_user_derivation > 0) {
            return Ok(Some(
                self.create_review(
                    "task_conflict",
                    "A Gmail inference conflicts with your loop",
                    "User-authored values outrank inferred values, so Kyra left the loop unchanged.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        if mode == PolicyMode::Passive && (extraction.truncated || proposal.ambiguity.is_some()) {
            return Ok(Some(
                self.create_review(
                    "task_ambiguous",
                    "Loop update needs review",
                    "The source was ambiguous or truncated.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let title = proposal
            .title
            .clone()
            .unwrap_or_else(|| before.title.clone());
        if title.trim().is_empty() || title.chars().count() > 240 {
            return Err(EngineError::Validation(
                "A loop title must be between 1 and 240 characters.".to_owned(),
            ));
        }
        let summary = proposal
            .summary
            .clone()
            .unwrap_or_else(|| before.summary.clone());
        let ownership = proposal
            .ownership
            .as_deref()
            .map(normalize_ownership)
            .transpose()?
            .unwrap_or(before.ownership.as_str());
        let due_at = proposal.due_at.clone().or_else(|| before.due_at.clone());
        validate_due_at(due_at.as_deref())?;
        let after = LoopSnapshot {
            id: before.id.clone(),
            title: title.clone(),
            summary: summary.clone(),
            lifecycle: before.lifecycle.clone(),
            ownership: ownership.to_owned(),
            priority: before.priority,
            due_at: due_at.clone(),
            version: before.version + 1,
        };
        let (nonce, ciphertext) = self.cipher.encrypt(&LoopPayload {
            title: title.clone(),
            summary: summary.clone(),
        })?;
        let action_id = stable_action_id(
            &self.cipher,
            "task-update",
            &before.id,
            &extraction.model_run_id,
        );
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE open_loops SET owner = ?, status = ?, ownership = ?, due_at = ?, payload_nonce = ?, payload_ciphertext = ?, version = version + 1, updated_at = ? WHERE id = ? AND version = ?")
            .bind(legacy_owner(ownership))
            .bind(legacy_status(ownership))
            .bind(ownership)
            .bind(&due_at)
            .bind(nonce)
            .bind(ciphertext)
            .bind(&now)
            .bind(&before.id)
            .bind(before.version)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(EngineError::Fenced);
        }
        self.persist_derivations(
            &mut transaction,
            &before.id,
            extraction,
            proposal,
            &title,
            &summary,
            ownership,
            mode,
        )
        .await?;
        self.persist_evidence(&mut transaction, &before.id, extraction, proposal, mode)
            .await?;
        let target_id = before.id.clone();
        self.persist_action(
            &mut transaction,
            &action_id,
            "task_update",
            &extraction.account_id,
            &extraction.model_run_id,
            Some(&target_id),
            None,
            Some(&after.version.to_string()),
            ActionRecord {
                title,
                detail: "Updated AI-derived loop fields from newer evidence.".to_owned(),
                before_loop: Some(before),
                after_loop: Some(after),
                before_calendar: None,
                after_calendar: None,
                operation_id: None,
                irreversible_effects: Vec::new(),
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn apply_task_resolution(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
    ) -> Result<Option<String>, EngineError> {
        let target = proposal.target_loop_id.as_deref().ok_or_else(|| {
            EngineError::Validation("Resolving a task needs an exact loop target.".to_owned())
        })?;
        let before = self.load_loop_snapshot(target).await?.ok_or_else(|| {
            EngineError::Validation("The target loop no longer exists.".to_owned())
        })?;
        let action_id = stable_action_id(
            &self.cipher,
            "task-resolve",
            target,
            &extraction.model_run_id,
        );
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE open_loops SET lifecycle = 'resolved', status = 'done', version = version + 1, updated_at = ? WHERE id = ? AND version = ?")
            .bind(&now)
            .bind(target)
            .bind(before.version)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(EngineError::Fenced);
        }
        let mut after = before.clone();
        after.lifecycle = "resolved".to_owned();
        after.version += 1;
        self.persist_action(
            &mut transaction,
            &action_id,
            "task_resolve",
            &extraction.account_id,
            &extraction.model_run_id,
            Some(target),
            None,
            Some(&after.version.to_string()),
            ActionRecord {
                title: after.title.clone(),
                detail: "Resolved a loop from an explicit command.".to_owned(),
                before_loop: Some(before),
                after_loop: Some(after),
                before_calendar: None,
                after_calendar: None,
                operation_id: None,
                irreversible_effects: Vec::new(),
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn apply_calendar_create(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        mode: PolicyMode,
    ) -> Result<Option<String>, EngineError> {
        let calendar = proposal.calendar.as_ref().ok_or_else(|| {
            EngineError::Validation("A Calendar proposal is missing event details.".to_owned())
        })?;
        if mode == PolicyMode::Passive
            && (!calendar_is_two_sided(extraction, proposal)
                || extraction.truncated
                || proposal.ambiguity.is_some())
        {
            return Ok(Some(
                self.create_review(
                    "calendar_ambiguous",
                    "Possible meeting",
                    "Kyra could not prove a complete two-sided meeting confirmation.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let title = calendar
            .title
            .clone()
            .or_else(|| proposal.title.clone())
            .ok_or_else(|| EngineError::Validation("The meeting needs a title.".to_owned()))?;
        let when = calendar_when(calendar)?;
        let attendees = self.person_emails(&calendar.attendee_person_ids).await?;
        if mode == PolicyMode::Passive && attendees.is_empty() {
            return Ok(Some(
                self.create_review(
                    "calendar_identity",
                    "Meeting attendees need review",
                    "Every attendee must resolve to one exact email identity.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let (start_at, end_at) = when_bounds(&when);
        let duplicate: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND starts_at = ? AND ends_at = ? AND status != 'cancelled'")
            .bind(&extraction.account_id)
            .bind(&start_at)
            .bind(&end_at)
            .fetch_one(&self.pool)
            .await?;
        if duplicate > 0 {
            return Ok(None);
        }
        let operation_id = self.cipher.pseudonymous_id(
            "calendar-create-operation",
            &format!(
                "{}:{}:{}:{}",
                extraction.account_id, extraction.thread_id, start_at, end_at
            ),
        );
        let send_updates = if mode == PolicyMode::Command && !attendees.is_empty() {
            "all"
        } else {
            "none"
        };
        let result = self
            .google
            .mutate_calendar(CalendarMutationInput::Create {
                operation_id: operation_id.clone(),
                event: CalendarEventInput {
                    title: title.clone(),
                    description: calendar.description.clone(),
                    location: calendar.location.clone(),
                    when,
                    attendees,
                    recurrence: calendar.recurrence.clone(),
                    send_updates: send_updates.to_owned(),
                },
            })
            .await?;
        let event = result.event.ok_or_else(|| {
            EngineError::Validation("Google did not return the created event.".to_owned())
        })?;
        let external_id = event.external_id.ok_or_else(|| {
            EngineError::Validation("Google did not return an event ID.".to_owned())
        })?;
        let snapshot = self
            .google
            .ai_calendar_snapshot(&extraction.account_id, &external_id)
            .await?;
        let action_id = stable_action_id(
            &self.cipher,
            "calendar-create",
            &external_id,
            &extraction.model_run_id,
        );
        let mut transaction = self.pool.begin().await?;
        let resulting_etag = snapshot.etag.clone();
        self.persist_action(
            &mut transaction,
            &action_id,
            "calendar_create",
            &extraction.account_id,
            &extraction.model_run_id,
            None,
            Some(&external_id),
            Some(&resulting_etag),
            ActionRecord {
                title,
                detail: "Created a silent Google Calendar event from a confirmed meeting."
                    .to_owned(),
                before_loop: None,
                after_loop: None,
                before_calendar: None,
                after_calendar: Some(snapshot),
                operation_id: Some(operation_id),
                irreversible_effects: Vec::new(),
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn apply_calendar_reschedule(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        mode: PolicyMode,
    ) -> Result<Option<String>, EngineError> {
        let calendar = proposal.calendar.as_ref().ok_or_else(|| {
            EngineError::Validation("A reschedule is missing Calendar details.".to_owned())
        })?;
        let event_id = calendar.event_id.as_deref().ok_or_else(|| {
            EngineError::Validation("A reschedule needs an exact event target.".to_owned())
        })?;
        let kyra_created: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_actions WHERE kind = 'calendar_create' AND target_event_id = ? AND status = 'succeeded'")
            .bind(event_id)
            .fetch_one(&self.pool)
            .await?;
        if mode == PolicyMode::Passive
            && (kyra_created == 0
                || !calendar_is_two_sided(extraction, proposal)
                || extraction.truncated
                || proposal.ambiguity.is_some())
        {
            return Ok(Some(
                self.create_review(
                    "calendar_reschedule",
                    "Reschedule needs review",
                    "Passive rescheduling is limited to unambiguous events previously created by Kyra.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let before = self
            .google
            .ai_calendar_snapshot(&extraction.account_id, event_id)
            .await?;
        if calendar
            .expected_etag
            .as_deref()
            .is_some_and(|etag| etag != before.etag)
        {
            return Ok(Some(
                self.create_review(
                    "calendar_stale",
                    "Calendar event changed elsewhere",
                    "Kyra synchronized the newer event and did not overwrite it.",
                    extraction,
                    proposal,
                    Vec::new(),
                )
                .await?,
            ));
        }
        let operation_id = self.cipher.pseudonymous_id(
            "calendar-update-operation",
            &format!(
                "{}:{}:{}",
                extraction.thread_id, event_id, extraction.model_run_id
            ),
        );
        let attendees = if mode == PolicyMode::Command {
            self.person_emails(&calendar.attendee_person_ids).await?
        } else {
            Vec::new()
        };
        let send_updates = if mode == PolicyMode::Command && !attendees.is_empty() {
            "all"
        } else {
            "none"
        };
        let result = self
            .google
            .mutate_calendar(CalendarMutationInput::Update {
                operation_id: operation_id.clone(),
                event_id: event_id.to_owned(),
                expected_etag: before.etag.clone(),
                patch: CalendarEventPatch {
                    title: calendar.title.clone(),
                    description: calendar.description.clone(),
                    location: calendar.location.clone(),
                    when: Some(calendar_when(calendar)?),
                    attendees: (!attendees.is_empty()).then_some(attendees),
                    recurrence: if calendar.recurrence.is_empty() {
                        None
                    } else {
                        Some(calendar.recurrence.clone())
                    },
                    send_updates: send_updates.to_owned(),
                },
            })
            .await?;
        let event = result.event.ok_or_else(|| {
            EngineError::Validation("Google did not return the updated event.".to_owned())
        })?;
        let external_id = event.external_id.unwrap_or_else(|| event_id.to_owned());
        let after = self
            .google
            .ai_calendar_snapshot(&extraction.account_id, &external_id)
            .await?;
        let action_id = stable_action_id(
            &self.cipher,
            "calendar-reschedule",
            event_id,
            &extraction.model_run_id,
        );
        let mut transaction = self.pool.begin().await?;
        let resulting_etag = after.etag.clone();
        self.persist_action(
            &mut transaction,
            &action_id,
            "calendar_update",
            &extraction.account_id,
            &extraction.model_run_id,
            None,
            Some(event_id),
            Some(&resulting_etag),
            ActionRecord {
                title: after.title.clone(),
                detail: "Rescheduled a Kyra-created event without notifying attendees.".to_owned(),
                before_loop: None,
                after_loop: None,
                before_calendar: Some(before),
                after_calendar: Some(after),
                operation_id: Some(operation_id),
                irreversible_effects: Vec::new(),
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn apply_calendar_delete(
        &self,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
    ) -> Result<Option<String>, EngineError> {
        let calendar = proposal.calendar.as_ref().ok_or_else(|| {
            EngineError::Validation("A Calendar deletion is missing its target.".to_owned())
        })?;
        let event_id = calendar.event_id.as_deref().ok_or_else(|| {
            EngineError::Validation("A Calendar deletion needs an exact event target.".to_owned())
        })?;
        let before = self
            .google
            .ai_calendar_snapshot(&extraction.account_id, event_id)
            .await?;
        if calendar
            .expected_etag
            .as_deref()
            .is_some_and(|etag| etag != before.etag)
        {
            return Err(EngineError::Validation(
                "The Calendar event changed elsewhere. Kyra synchronized it and did not delete it."
                    .to_owned(),
            ));
        }
        let operation_id = self.cipher.pseudonymous_id(
            "calendar-delete-operation",
            &format!(
                "{}:{}:{}",
                extraction.thread_id, event_id, extraction.model_run_id
            ),
        );
        self.google
            .mutate_calendar(CalendarMutationInput::Delete {
                operation_id: operation_id.clone(),
                event_id: event_id.to_owned(),
                expected_etag: before.etag.clone(),
                send_updates: "none".to_owned(),
            })
            .await?;
        let action_id = stable_action_id(
            &self.cipher,
            "calendar-delete",
            event_id,
            &extraction.model_run_id,
        );
        let mut transaction = self.pool.begin().await?;
        self.persist_action(
            &mut transaction,
            &action_id,
            "calendar_delete",
            &extraction.account_id,
            &extraction.model_run_id,
            None,
            Some(event_id),
            None,
            ActionRecord {
                title: before.title.clone(),
                detail: "Deleted a Google Calendar event without notifying attendees.".to_owned(),
                before_loop: None,
                after_loop: None,
                before_calendar: Some(before),
                after_calendar: None,
                operation_id: Some(operation_id),
                irreversible_effects: vec![
                    "A recreated event receives a new Google event ID; deleted Meet links and organizer metadata cannot be restored."
                        .to_owned(),
                ],
            },
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(action_id))
    }

    async fn create_review(
        &self,
        kind: &str,
        title: &str,
        summary: &str,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        irreversible_effects: Vec<String>,
    ) -> Result<String, EngineError> {
        let review_id = format!(
            "review_{}",
            &self.cipher.pseudonymous_id(
                "ai-review",
                &format!(
                    "{}:{}:{}",
                    extraction.model_run_id, proposal.proposal_id, kind
                )
            )[..24]
        );
        let record = ReviewRecord {
            title: title.to_owned(),
            summary: summary.to_owned(),
            evidence: evidence_quotes(extraction, proposal),
            irreversible_effects,
            proposal: proposal.clone(),
            extraction: extraction.clone(),
        };
        let (nonce, ciphertext) = self.cipher.encrypt(&record)?;
        sqlx::query("INSERT OR IGNORE INTO ai_reviews (id, kind, status, account_id, account_generation, source_revision_id, model_run_id, target_loop_id, target_event_id, payload_nonce, payload_ciphertext, created_at) VALUES (?, ?, 'pending', ?, (SELECT generation FROM connector_accounts WHERE id = ?), ?, ?, ?, ?, ?, ?, ?)")
            .bind(&review_id)
            .bind(kind)
            .bind(&extraction.account_id)
            .bind(&extraction.account_id)
            .bind(proposal.evidence.first().map(|item| item.source_revision_id.as_str()))
            .bind(&extraction.model_run_id)
            .bind(&proposal.target_loop_id)
            .bind(proposal.calendar.as_ref().and_then(|calendar| calendar.event_id.as_deref()))
            .bind(nonce)
            .bind(ciphertext)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(review_id)
    }

    pub(super) async fn list_reviews(&self) -> Result<Vec<AiReviewItem>, EngineError> {
        let rows = sqlx::query_as::<_, (String, String, Vec<u8>, Vec<u8>, String)>("SELECT id, kind, payload_nonce, payload_ciphertext, created_at FROM ai_reviews WHERE status = 'pending' ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(id, kind, nonce, ciphertext, created_at)| {
                let payload: ReviewRecord = self.cipher.decrypt(&nonce, &ciphertext)?;
                Ok(AiReviewItem {
                    id,
                    kind,
                    title: payload.title,
                    summary: payload.summary,
                    evidence: payload.evidence,
                    irreversible_effects: payload.irreversible_effects,
                    created_at,
                })
            })
            .collect()
    }

    pub(super) async fn list_activity(&self) -> Result<Vec<AiActivityItem>, EngineError> {
        let rows = sqlx::query_as::<_, (String, String, String, Vec<u8>, Vec<u8>, String)>("SELECT id, kind, status, payload_nonce, payload_ciphertext, created_at FROM ai_actions ORDER BY created_at DESC LIMIT 100")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(id, kind, status, nonce, ciphertext, created_at)| {
                let payload: ActionRecord = self.cipher.decrypt(&nonce, &ciphertext)?;
                let compensation = kind.starts_with("calendar_");
                Ok(AiActivityItem {
                    id,
                    kind,
                    status: status.clone(),
                    title: payload.title,
                    detail: payload.detail,
                    can_revert: status == "succeeded",
                    compensation,
                    irreversible_effects: payload.irreversible_effects,
                    created_at,
                })
            })
            .collect()
    }

    pub(super) async fn resolve_review_policy(
        &self,
        review_id: &str,
        decision: &str,
    ) -> Result<Vec<String>, EngineError> {
        if decision == "dismiss" {
            let updated = sqlx::query("UPDATE ai_reviews SET status = 'dismissed', resolved_at = ? WHERE id = ? AND status = 'pending'")
                .bind(Utc::now().to_rfc3339())
                .bind(review_id)
                .execute(&self.pool)
                .await?;
            if updated.rows_affected() != 1 {
                return Err(EngineError::Validation(
                    "That review is no longer pending.".to_owned(),
                ));
            }
            return Ok(Vec::new());
        }
        if decision != "accept" {
            return Err(EngineError::Validation(
                "Choose accept or dismiss.".to_owned(),
            ));
        }
        let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>("SELECT payload_nonce, payload_ciphertext FROM ai_reviews WHERE id = ? AND status = 'pending'")
            .bind(review_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| EngineError::Validation("That review is no longer pending.".to_owned()))?;
        let record: ReviewRecord = self.cipher.decrypt(&row.0, &row.1)?;
        let mut extraction = record.extraction;
        extraction.envelope.proposals = vec![record.proposal];
        let actions = self
            .apply_extraction(&extraction, PolicyMode::Reviewed)
            .await?;
        sqlx::query("UPDATE ai_reviews SET status = 'accepted', resolved_at = ? WHERE id = ? AND status = 'pending'")
            .bind(Utc::now().to_rfc3339())
            .bind(review_id)
            .execute(&self.pool)
            .await?;
        Ok(actions)
    }

    pub(super) async fn revert_action_policy(
        &self,
        action_id: &str,
    ) -> Result<(String, String), EngineError> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, Vec<u8>, Vec<u8>)>("SELECT kind, account_id, target_loop_id, target_event_id, resulting_version, payload_nonce, payload_ciphertext FROM ai_actions WHERE id = ? AND status = 'succeeded'")
            .bind(action_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| EngineError::Validation("That action can no longer be reverted.".to_owned()))?;
        let (
            kind,
            _account_id,
            target_loop_id,
            target_event_id,
            resulting_version,
            nonce,
            ciphertext,
        ) = row;
        let payload: ActionRecord = self.cipher.decrypt(&nonce, &ciphertext)?;
        match kind.as_str() {
            "task_create" => {
                let target = target_loop_id.ok_or(EngineError::Fenced)?;
                let expected = parse_resulting_version(resulting_version.as_deref())?;
                let deleted = sqlx::query("DELETE FROM open_loops WHERE id = ? AND version = ?")
                    .bind(target)
                    .bind(expected)
                    .execute(&self.pool)
                    .await?;
                if deleted.rows_affected() != 1 {
                    return self.mark_action_conflict(action_id).await;
                }
                self.mark_action_reverted(action_id, "reverted").await?;
                Ok((
                    "reverted".to_owned(),
                    "Removed the AI-created loop.".to_owned(),
                ))
            }
            "task_update" | "task_resolve" => {
                let before = payload.before_loop.ok_or(EngineError::Fenced)?;
                let expected = parse_resulting_version(resulting_version.as_deref())?;
                let (payload_nonce, payload_ciphertext) = self.cipher.encrypt(&LoopPayload {
                    title: before.title.clone(),
                    summary: before.summary.clone(),
                })?;
                let status = if before.lifecycle == "resolved" {
                    "done"
                } else {
                    legacy_status(&before.ownership)
                };
                let updated = sqlx::query("UPDATE open_loops SET lifecycle = ?, ownership = ?, owner = ?, status = ?, priority = ?, due_at = ?, payload_nonce = ?, payload_ciphertext = ?, version = version + 1, updated_at = ? WHERE id = ? AND version = ?")
                    .bind(&before.lifecycle)
                    .bind(&before.ownership)
                    .bind(legacy_owner(&before.ownership))
                    .bind(status)
                    .bind(before.priority)
                    .bind(&before.due_at)
                    .bind(payload_nonce)
                    .bind(payload_ciphertext)
                    .bind(Utc::now().to_rfc3339())
                    .bind(&before.id)
                    .bind(expected)
                    .execute(&self.pool)
                    .await?;
                if updated.rows_affected() != 1 {
                    return self.mark_action_conflict(action_id).await;
                }
                self.mark_action_reverted(action_id, "reverted").await?;
                Ok((
                    "reverted".to_owned(),
                    "Restored the previous loop snapshot.".to_owned(),
                ))
            }
            "calendar_create" => {
                let event_id = target_event_id.ok_or(EngineError::Fenced)?;
                let expected_etag = resulting_version.ok_or(EngineError::Fenced)?;
                self.google
                    .mutate_calendar(CalendarMutationInput::Delete {
                        operation_id: format!("compensate-{action_id}"),
                        event_id,
                        expected_etag,
                        send_updates: "none".to_owned(),
                    })
                    .await?;
                self.mark_action_reverted(action_id, "compensated").await?;
                Ok((
                    "compensated".to_owned(),
                    "Deleted the unchanged event Kyra created.".to_owned(),
                ))
            }
            "calendar_update" => {
                let event_id = target_event_id.ok_or(EngineError::Fenced)?;
                let expected_etag = resulting_version.ok_or(EngineError::Fenced)?;
                let before = payload.before_calendar.ok_or(EngineError::Fenced)?;
                self.google
                    .mutate_calendar(CalendarMutationInput::Update {
                        operation_id: format!("compensate-{action_id}"),
                        event_id,
                        expected_etag,
                        patch: snapshot_patch(&before),
                    })
                    .await?;
                self.mark_action_reverted(action_id, "compensated").await?;
                Ok((
                    "compensated".to_owned(),
                    "Applied the previous supported event snapshot.".to_owned(),
                ))
            }
            "calendar_delete" => {
                let before = payload.before_calendar.ok_or(EngineError::Fenced)?;
                self.google
                    .mutate_calendar(CalendarMutationInput::Create {
                        operation_id: format!("compensate-{action_id}"),
                        event: snapshot_input(&before),
                    })
                    .await?;
                self.mark_action_reverted(action_id, "compensated").await?;
                Ok((
                    "compensated".to_owned(),
                    "Recreated the supported event snapshot with a new Google event ID.".to_owned(),
                ))
            }
            _ => Err(EngineError::Validation(
                "This activity does not support reversion.".to_owned(),
            )),
        }
    }

    async fn mark_action_reverted(&self, id: &str, status: &str) -> Result<(), EngineError> {
        sqlx::query("UPDATE ai_actions SET status = ?, reverted_at = ? WHERE id = ? AND status = 'succeeded'")
            .bind(status)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_action_conflict<T>(&self, id: &str) -> Result<T, EngineError> {
        sqlx::query(
            "UPDATE ai_actions SET status = 'conflict' WHERE id = ? AND status = 'succeeded'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Err(EngineError::Validation(
            "The item changed after this action, so Kyra did not overwrite the newer state."
                .to_owned(),
        ))
    }

    async fn load_loop_snapshot(&self, id: &str) -> Result<Option<LoopSnapshot>, EngineError> {
        let row = sqlx::query_as::<_, (String, String, String, i64, Option<String>, i64, Vec<u8>, Vec<u8>)>("SELECT id, lifecycle, ownership, priority, due_at, version, payload_nonce, payload_ciphertext FROM open_loops WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(
            |(id, lifecycle, ownership, priority, due_at, version, nonce, ciphertext)| {
                let payload: LoopPayload = self.cipher.decrypt(&nonce, &ciphertext)?;
                Ok(LoopSnapshot {
                    id,
                    title: payload.title,
                    summary: payload.summary,
                    lifecycle,
                    ownership,
                    priority,
                    due_at,
                    version,
                })
            },
        )
        .transpose()
    }

    #[allow(clippy::too_many_arguments)]
    async fn persist_derivations(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        loop_id: &str,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        title: &str,
        summary: &str,
        ownership: &str,
        mode: PolicyMode,
    ) -> Result<(), EngineError> {
        let source_type = if mode == PolicyMode::Command {
            "command"
        } else {
            "google"
        };
        let source_revision_id = if mode == PolicyMode::Command {
            None
        } else {
            proposal
                .evidence
                .first()
                .map(|evidence| evidence.source_revision_id.as_str())
        };
        sqlx::query("UPDATE loop_derivations SET active = 0 WHERE loop_id = ? AND source_type = ? AND field_name IN ('title', 'summary', 'ownership', 'due_at')")
            .bind(loop_id)
            .bind(source_type)
            .execute(&mut **transaction)
            .await?;
        for (field, value) in [
            ("title", title.to_owned()),
            ("summary", summary.to_owned()),
            ("ownership", ownership.to_owned()),
            ("due_at", proposal.due_at.clone().unwrap_or_default()),
        ] {
            let (nonce, ciphertext) = self.cipher.encrypt(&value)?;
            sqlx::query("INSERT INTO loop_derivations (id, loop_id, field_name, source_type, source_revision_id, model_run_id, active, value_hash, payload_nonce, payload_ciphertext, created_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)")
                .bind(Uuid::new_v4().to_string())
                .bind(loop_id)
                .bind(field)
                .bind(source_type)
                .bind(source_revision_id)
                .bind(&extraction.model_run_id)
                .bind(self.cipher.pseudonymous_id("derivation-value", &value))
                .bind(nonce)
                .bind(ciphertext)
                .bind(Utc::now().to_rfc3339())
                .execute(&mut **transaction)
                .await?;
        }
        Ok(())
    }

    async fn persist_evidence(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        loop_id: &str,
        extraction: &CompletedExtraction,
        proposal: &IntentProposal,
        mode: PolicyMode,
    ) -> Result<(), EngineError> {
        for evidence in &proposal.evidence {
            let excerpt = exact_quote(extraction, evidence.start_offset, evidence.end_offset)
                .unwrap_or_default();
            let payload = EvidencePayload {
                source_label: if mode == PolicyMode::Command {
                    "Command+K".to_owned()
                } else {
                    "Gmail thread".to_owned()
                },
                excerpt,
            };
            let (nonce, ciphertext) = self.cipher.encrypt(&payload)?;
            let id = format!(
                "evidence_{}",
                &self.cipher.pseudonymous_id(
                    "loop-evidence",
                    &format!(
                        "{}:{}:{}:{}",
                        loop_id,
                        evidence.source_revision_id,
                        evidence.start_offset,
                        evidence.end_offset
                    )
                )[..24]
            );
            let source_revision_id =
                (mode != PolicyMode::Command).then_some(evidence.source_revision_id.as_str());
            let source_kind = if mode == PolicyMode::Command {
                "command"
            } else {
                "gmail"
            };
            sqlx::query("INSERT OR IGNORE INTO evidence (id, loop_id, source_kind, source_label, excerpt, occurred_at, source_revision_id, document_hash, start_offset, end_offset, quote_hash, payload_nonce, payload_ciphertext, payload_migrated) VALUES (?, ?, ?, '', '', ?, ?, ?, ?, ?, ?, ?, ?, 1)")
                .bind(id)
                .bind(loop_id)
                .bind(source_kind)
                .bind(Utc::now().to_rfc3339())
                .bind(source_revision_id)
                .bind(&evidence.document_hash)
                .bind(evidence.start_offset as i64)
                .bind(evidence.end_offset as i64)
                .bind(&evidence.quote_hash)
                .bind(nonce)
                .bind(ciphertext)
                .execute(&mut **transaction)
                .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn persist_action(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        id: &str,
        kind: &str,
        account_id: &str,
        model_run_id: &str,
        target_loop_id: Option<&str>,
        target_event_id: Option<&str>,
        resulting_version: Option<&str>,
        payload: ActionRecord,
    ) -> Result<(), EngineError> {
        let irreversible = !payload.irreversible_effects.is_empty();
        let (nonce, ciphertext) = self.cipher.encrypt(&payload)?;
        let account_id = (!account_id.is_empty()).then_some(account_id);
        sqlx::query("INSERT OR IGNORE INTO ai_actions (id, kind, status, account_id, account_generation, model_run_id, target_loop_id, target_event_id, resulting_version, irreversible_effects, payload_nonce, payload_ciphertext, created_at) VALUES (?, ?, 'succeeded', ?, (SELECT generation FROM connector_accounts WHERE id = ?), ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(id)
            .bind(kind)
            .bind(account_id)
            .bind(account_id)
            .bind(model_run_id)
            .bind(target_loop_id)
            .bind(target_event_id)
            .bind(resulting_version)
            .bind(i64::from(irreversible))
            .bind(nonce)
            .bind(ciphertext)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    pub(super) async fn person_emails(
        &self,
        person_ids: &[String],
    ) -> Result<Vec<String>, EngineError> {
        let mut emails = Vec::with_capacity(person_ids.len());
        let mut unique = HashSet::new();
        for person_id in person_ids {
            let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
                "SELECT payload_nonce, payload_ciphertext FROM ai_people WHERE id = ?",
            )
            .bind(person_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| {
                EngineError::Validation("A Calendar attendee is ambiguous.".to_owned())
            })?;
            let payload: super::runtime::PersonPayload = self.cipher.decrypt(&row.0, &row.1)?;
            if !payload.is_me && unique.insert(payload.email.clone()) {
                emails.push(payload.email);
            }
        }
        Ok(emails)
    }
}

fn required_title(proposal: &IntentProposal) -> Result<String, EngineError> {
    let title = proposal.title.as_deref().map(str::trim).unwrap_or_default();
    if title.is_empty() || title.chars().count() > 240 {
        return Err(EngineError::Validation(
            "A proposed loop needs a concise title.".to_owned(),
        ));
    }
    Ok(title.to_owned())
}

fn normalize_ownership(value: &str) -> Result<&str, EngineError> {
    match value {
        "me" | "other" | "shared" | "unknown" => Ok(value),
        _ => Err(EngineError::Validation(
            "The proposed loop ownership is invalid.".to_owned(),
        )),
    }
}

fn legacy_owner(ownership: &str) -> &str {
    if ownership == "other" {
        "them"
    } else {
        "me"
    }
}

fn legacy_status(ownership: &str) -> &str {
    if ownership == "other" {
        "waiting"
    } else {
        "open"
    }
}

fn validate_due_at(value: Option<&str>) -> Result<(), EngineError> {
    if let Some(value) = value {
        DateTime::parse_from_rfc3339(value)
            .map_err(|_| EngineError::Validation("The proposed due date is invalid.".to_owned()))?;
    }
    Ok(())
}

fn calendar_is_two_sided(extraction: &CompletedExtraction, proposal: &IntentProposal) -> bool {
    proposal
        .evidence
        .iter()
        .filter_map(|evidence| extraction.source_people.get(&evidence.source_revision_id))
        .collect::<HashSet<_>>()
        .len()
        >= 2
}

fn calendar_when(calendar: &super::types::CalendarProposal) -> Result<CalendarWhen, EngineError> {
    if let (Some(start_at), Some(end_at), Some(time_zone)) = (
        calendar.start_at.clone(),
        calendar.end_at.clone(),
        calendar.time_zone.clone(),
    ) {
        return Ok(CalendarWhen::Timed {
            start_at,
            end_at,
            time_zone,
        });
    }
    if let (Some(start_date), Some(end_date)) =
        (calendar.all_day_start.clone(), calendar.all_day_end.clone())
    {
        return Ok(CalendarWhen::AllDay {
            start_date,
            end_date,
        });
    }
    Err(EngineError::Validation(
        "The meeting needs an explicit start, end, and time zone.".to_owned(),
    ))
}

fn when_bounds(when: &CalendarWhen) -> (String, String) {
    match when {
        CalendarWhen::Timed {
            start_at, end_at, ..
        } => (start_at.clone(), end_at.clone()),
        CalendarWhen::AllDay {
            start_date,
            end_date,
        } => (
            format!("{start_date}T00:00:00Z"),
            format!("{end_date}T00:00:00Z"),
        ),
    }
}

fn snapshot_when(snapshot: &AiCalendarSnapshot) -> CalendarWhen {
    if snapshot.all_day {
        CalendarWhen::AllDay {
            start_date: snapshot.start_at.chars().take(10).collect(),
            end_date: snapshot.end_at.chars().take(10).collect(),
        }
    } else {
        CalendarWhen::Timed {
            start_at: snapshot.start_at.clone(),
            end_at: snapshot.end_at.clone(),
            time_zone: snapshot
                .time_zone
                .clone()
                .unwrap_or_else(|| "UTC".to_owned()),
        }
    }
}

fn snapshot_input(snapshot: &AiCalendarSnapshot) -> CalendarEventInput {
    CalendarEventInput {
        title: snapshot.title.clone(),
        description: snapshot.description.clone(),
        location: snapshot.location.clone(),
        when: snapshot_when(snapshot),
        attendees: snapshot.attendees.clone(),
        recurrence: snapshot.recurrence.clone(),
        send_updates: "none".to_owned(),
    }
}

fn snapshot_patch(snapshot: &AiCalendarSnapshot) -> CalendarEventPatch {
    CalendarEventPatch {
        title: Some(snapshot.title.clone()),
        description: snapshot.description.clone(),
        location: snapshot.location.clone(),
        when: Some(snapshot_when(snapshot)),
        attendees: Some(snapshot.attendees.clone()),
        recurrence: Some(snapshot.recurrence.clone()),
        send_updates: "none".to_owned(),
    }
}

fn parse_resulting_version(value: Option<&str>) -> Result<i64, EngineError> {
    value
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(EngineError::Fenced)
}

fn exact_quote(extraction: &CompletedExtraction, start: usize, end: usize) -> Option<String> {
    extraction
        .document_text
        .as_bytes()
        .get(start..end)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(str::to_owned)
}

fn evidence_quotes(extraction: &CompletedExtraction, proposal: &IntentProposal) -> Vec<String> {
    proposal
        .evidence
        .iter()
        .filter_map(|evidence| exact_quote(extraction, evidence.start_offset, evidence.end_offset))
        .collect()
}

fn stable_action_id(
    cipher: &crate::crypto::LocalCipher,
    kind: &str,
    target: &str,
    model_run_id: &str,
) -> String {
    let stable = cipher.pseudonymous_id("ai-action", &format!("{kind}:{target}:{model_run_id}"));
    format!("action_{}", &stable[..24])
}

#[cfg(test)]
pub(super) fn quote_hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_calendar_requires_two_distinct_people() {
        let mut extraction = CompletedExtraction {
            account_id: "account".to_owned(),
            thread_id: "thread".to_owned(),
            envelope: super::super::types::IntentEnvelope {
                schema_version: "kyra.intent.v1".to_owned(),
                activation_fingerprint: "fp".to_owned(),
                source_document_hash: "hash".to_owned(),
                proposals: Vec::new(),
            },
            document_text: "proposal acceptance".to_owned(),
            document_hash: "hash".to_owned(),
            source_revision_ids: vec!["r1".to_owned(), "r2".to_owned()],
            source_people: [("r1".to_owned(), "p1".to_owned())].into_iter().collect(),
            truncated: false,
            model_run_id: "run".to_owned(),
        };
        let proposal = IntentProposal {
            proposal_id: "p".to_owned(),
            action: IntentAction::CalendarCreate,
            target_loop_id: None,
            title: None,
            summary: None,
            ownership: None,
            due_at: None,
            calendar: None,
            person_ids: Vec::new(),
            fact_ids: Vec::new(),
            evidence: vec![
                super::super::types::EvidenceReference {
                    source_revision_id: "r1".to_owned(),
                    document_hash: "hash".to_owned(),
                    start_offset: 0,
                    end_offset: 8,
                    quote_hash: quote_hash("proposal"),
                },
                super::super::types::EvidenceReference {
                    source_revision_id: "r2".to_owned(),
                    document_hash: "hash".to_owned(),
                    start_offset: 9,
                    end_offset: 19,
                    quote_hash: quote_hash("acceptance"),
                },
            ],
            confidence: 1.0,
            ambiguity: None,
        };
        assert!(!calendar_is_two_sided(&extraction, &proposal));
        extraction
            .source_people
            .insert("r2".to_owned(), "p2".to_owned());
        assert!(calendar_is_two_sided(&extraction, &proposal));
    }
}
