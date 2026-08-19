use std::collections::HashSet;

use chrono::{DateTime, NaiveDate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::types::{CanonicalDocument, IntentAction, IntentEnvelope, INTENT_SCHEMA_VERSION};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContractError {
    #[error("The model returned an unsupported schema version.")]
    SchemaVersion,
    #[error("The model result belongs to an inactive configuration.")]
    ActivationFingerprint,
    #[error("The source changed while the model was running.")]
    DocumentHash,
    #[error("The model referenced an unknown Kyra record.")]
    UnknownReference,
    #[error("The model cited evidence that is not present in the source.")]
    InvalidEvidence,
    #[error("The model returned an unsupported or invalid action.")]
    InvalidAction,
}

pub struct ValidationContext<'a> {
    pub activation_fingerprint: &'a str,
    pub document: &'a CanonicalDocument,
    pub known_loop_ids: &'a HashSet<String>,
    pub known_event_ids: &'a HashSet<String>,
    pub known_person_ids: &'a HashSet<String>,
    pub known_fact_ids: &'a HashSet<String>,
}

pub fn validate_envelope(
    envelope: &IntentEnvelope,
    context: &ValidationContext<'_>,
) -> Result<(), ContractError> {
    if envelope.schema_version != INTENT_SCHEMA_VERSION {
        return Err(ContractError::SchemaVersion);
    }
    if envelope.activation_fingerprint != context.activation_fingerprint {
        return Err(ContractError::ActivationFingerprint);
    }
    if envelope.source_document_hash != context.document.document_hash {
        return Err(ContractError::DocumentHash);
    }
    for proposal in &envelope.proposals {
        if !(0.0..=1.0).contains(&proposal.confidence) || proposal.proposal_id.trim().is_empty() {
            return Err(ContractError::InvalidAction);
        }
        if let Some(loop_id) = proposal.target_loop_id.as_deref() {
            if !context.known_loop_ids.contains(loop_id) {
                return Err(ContractError::UnknownReference);
            }
        }
        if proposal
            .person_ids
            .iter()
            .any(|id| !context.known_person_ids.contains(id))
        {
            return Err(ContractError::UnknownReference);
        }
        if proposal
            .fact_ids
            .iter()
            .any(|id| !context.known_fact_ids.contains(id))
        {
            return Err(ContractError::UnknownReference);
        }
        if proposal.briefing_segments.iter().any(|segment| {
            !context.known_fact_ids.contains(&segment.fact_id)
                || !context.known_loop_ids.contains(&segment.subject_loop_id)
        }) {
            return Err(ContractError::UnknownReference);
        }
        if matches!(proposal.action, IntentAction::BriefingOrder) {
            let segment_facts: Vec<&str> = proposal
                .briefing_segments
                .iter()
                .map(|segment| segment.fact_id.as_str())
                .collect();
            let unique: HashSet<&str> = segment_facts.iter().copied().collect();
            if proposal
                .fact_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != segment_facts
                || unique.len() != segment_facts.len()
                || proposal.briefing_segments.is_empty()
            {
                return Err(ContractError::InvalidAction);
            }
        } else if !proposal.briefing_segments.is_empty() {
            return Err(ContractError::InvalidAction);
        }
        if let Some(calendar) = proposal.calendar.as_ref() {
            if calendar
                .attendee_person_ids
                .iter()
                .any(|id| !context.known_person_ids.contains(id))
            {
                return Err(ContractError::UnknownReference);
            }
            if let Some(event_id) = calendar.event_id.as_deref() {
                if !context.known_event_ids.contains(event_id)
                    && !matches!(proposal.action, IntentAction::CalendarCreate)
                {
                    return Err(ContractError::UnknownReference);
                }
            }
            validate_calendar(calendar)?;
        }
        if !matches!(
            proposal.action,
            IntentAction::NoAction | IntentAction::BriefingOrder
        ) && proposal.evidence.is_empty()
        {
            return Err(ContractError::InvalidEvidence);
        }
        for evidence in &proposal.evidence {
            if evidence.document_hash != context.document.document_hash
                || !context
                    .document
                    .source_revision_ids
                    .contains(&evidence.source_revision_id)
                || evidence.start_offset >= evidence.end_offset
                || evidence.end_offset > context.document.text.len()
                || !context
                    .document
                    .text
                    .is_char_boundary(evidence.start_offset)
                || !context.document.text.is_char_boundary(evidence.end_offset)
            {
                return Err(ContractError::InvalidEvidence);
            }
            let quote = &context.document.text[evidence.start_offset..evidence.end_offset];
            if evidence.quote_hash != sha256_hex(quote.as_bytes()) {
                return Err(ContractError::InvalidEvidence);
            }
            let mapped = context.document.span_map.iter().any(|span| {
                span.source_revision_id == evidence.source_revision_id
                    && evidence.start_offset >= span.transformed_start
                    && evidence.end_offset <= span.transformed_end
            });
            if !mapped {
                return Err(ContractError::InvalidEvidence);
            }
        }
    }
    Ok(())
}

fn validate_calendar(calendar: &super::types::CalendarProposal) -> Result<(), ContractError> {
    if let Some(updates) = calendar.send_updates.as_deref() {
        if !matches!(updates, "all" | "externalOnly" | "none") {
            return Err(ContractError::InvalidAction);
        }
    }
    match (
        calendar.start_at.as_deref(),
        calendar.end_at.as_deref(),
        calendar.all_day_start.as_deref(),
        calendar.all_day_end.as_deref(),
    ) {
        (Some(start), Some(end), None, None) => {
            let start =
                DateTime::parse_from_rfc3339(start).map_err(|_| ContractError::InvalidAction)?;
            let end =
                DateTime::parse_from_rfc3339(end).map_err(|_| ContractError::InvalidAction)?;
            if end <= start || calendar.time_zone.as_deref().unwrap_or_default().is_empty() {
                return Err(ContractError::InvalidAction);
            }
        }
        (None, None, Some(start), Some(end)) => {
            let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")
                .map_err(|_| ContractError::InvalidAction)?;
            let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")
                .map_err(|_| ContractError::InvalidAction)?;
            if end <= start {
                return Err(ContractError::InvalidAction);
            }
        }
        (None, None, None, None) => {}
        _ => return Err(ContractError::InvalidAction),
    }
    Ok(())
}

pub fn quote_hash(quote: &str) -> String {
    sha256_hex(quote.as_bytes())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn intent_json_schema() -> Value {
    let nullable_string = || json!({"type": ["string", "null"]});
    let string_array = || json!({"type": "array", "items": {"type": "string"}});
    let evidence = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "sourceRevisionId": {"type": "string"},
            "documentHash": {"type": "string"},
            "startOffset": {"type": "integer", "minimum": 0},
            "endOffset": {"type": "integer", "minimum": 1},
            "quoteHash": {"type": "string"}
        },
        "required": ["sourceRevisionId", "documentHash", "startOffset", "endOffset", "quoteHash"]
    });
    let calendar = json!({
        "type": ["object", "null"],
        "additionalProperties": false,
        "properties": {
            "eventId": nullable_string(),
            "title": nullable_string(),
            "description": nullable_string(),
            "location": nullable_string(),
            "startAt": nullable_string(),
            "endAt": nullable_string(),
            "allDayStart": nullable_string(),
            "allDayEnd": nullable_string(),
            "timeZone": nullable_string(),
            "attendeePersonIds": string_array(),
            "recurrence": string_array(),
            "expectedEtag": nullable_string(),
            "sendUpdates": {"type": ["string", "null"], "enum": ["all", "externalOnly", "none", null]}
        },
        "required": ["eventId", "title", "description", "location", "startAt", "endAt", "allDayStart", "allDayEnd", "timeZone", "attendeePersonIds", "recurrence", "expectedEtag", "sendUpdates"]
    });
    let briefing_segment = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "factId": {"type": "string"},
            "subjectLoopId": {"type": "string"},
            "narrativeRole": {"type": "string", "enum": ["on_me", "waiting", "shared", "scheduled", "needs_review"]},
            "urgency": {"type": "string", "enum": ["low", "medium", "high"]},
            "actionReference": {"type": "string", "enum": ["protect_time", "follow_up", "coordinate", "attend", "review"]}
        },
        "required": ["factId", "subjectLoopId", "narrativeRole", "urgency", "actionReference"]
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "schemaVersion": {"type": "string", "const": INTENT_SCHEMA_VERSION},
            "activationFingerprint": {"type": "string"},
            "sourceDocumentHash": {"type": "string"},
            "proposals": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "proposalId": {"type": "string"},
                        "action": {"type": "string", "enum": ["task_create", "task_update", "resolution_suggest", "calendar_create", "calendar_reschedule", "calendar_cancel", "calendar_delete", "briefing_order", "no_action"]},
                        "targetLoopId": nullable_string(),
                        "title": nullable_string(),
                        "summary": nullable_string(),
                        "ownership": {"type": ["string", "null"], "enum": ["me", "other", "shared", "unknown", null]},
                        "dueAt": nullable_string(),
                        "calendar": calendar,
                        "personIds": string_array(),
                        "factIds": string_array(),
                        "briefingSegments": {"type": "array", "items": briefing_segment},
                        "evidence": {"type": "array", "items": evidence},
                        "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                        "ambiguity": nullable_string()
                    },
                    "required": ["proposalId", "action", "targetLoopId", "title", "summary", "ownership", "dueAt", "calendar", "personIds", "factIds", "briefingSegments", "evidence", "confidence", "ambiguity"]
                }
            }
        },
        "required": ["schemaVersion", "activationFingerprint", "sourceDocumentHash", "proposals"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{EvidenceReference, IntentProposal};

    fn context() -> (CanonicalDocument, HashSet<String>) {
        let text = "[message m1 person p1]\nPlease send the report tomorrow.".to_owned();
        let start = text.find("Please").unwrap();
        let end = text.len();
        (
            CanonicalDocument {
                document_hash: sha256_hex(text.as_bytes()),
                text,
                truncated: false,
                span_map: vec![super::super::types::SpanMap {
                    source_revision_id: "r1".to_owned(),
                    original_start: 0,
                    original_end: end - start,
                    transformed_start: start,
                    transformed_end: end,
                }],
                source_revision_ids: vec!["r1".to_owned()],
                person_ids: vec!["p1".to_owned()],
            },
            HashSet::from(["p1".to_owned()]),
        )
    }

    #[test]
    fn validates_exact_evidence_and_rejects_tampering() {
        let (document, people) = context();
        let start = document.text.find("Please").unwrap();
        let quote = &document.text[start..];
        let mut envelope = IntentEnvelope {
            schema_version: INTENT_SCHEMA_VERSION.to_owned(),
            activation_fingerprint: "active".to_owned(),
            source_document_hash: document.document_hash.clone(),
            proposals: vec![IntentProposal {
                proposal_id: "p".to_owned(),
                action: IntentAction::TaskCreate,
                target_loop_id: None,
                title: Some("Send report".to_owned()),
                summary: None,
                ownership: Some("other".to_owned()),
                due_at: None,
                calendar: None,
                person_ids: vec!["p1".to_owned()],
                fact_ids: Vec::new(),
                briefing_segments: Vec::new(),
                evidence: vec![EvidenceReference {
                    source_revision_id: "r1".to_owned(),
                    document_hash: document.document_hash.clone(),
                    start_offset: start,
                    end_offset: document.text.len(),
                    quote_hash: quote_hash(quote),
                }],
                confidence: 0.9,
                ambiguity: None,
            }],
        };
        let context = ValidationContext {
            activation_fingerprint: "active",
            document: &document,
            known_loop_ids: &HashSet::new(),
            known_event_ids: &HashSet::new(),
            known_person_ids: &people,
            known_fact_ids: &HashSet::new(),
        };
        assert_eq!(validate_envelope(&envelope, &context), Ok(()));
        envelope.proposals[0].evidence[0].quote_hash = "tampered".to_owned();
        assert_eq!(
            validate_envelope(&envelope, &context),
            Err(ContractError::InvalidEvidence)
        );
    }

    #[test]
    fn schema_forbids_unknown_properties() {
        let schema = intent_json_schema();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["proposals"]["items"]["additionalProperties"],
            false
        );
    }
}
