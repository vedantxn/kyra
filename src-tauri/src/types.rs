use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub source_kind: String,
    pub source_label: String,
    pub excerpt: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopPayload {
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidencePayload {
    pub source_label: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransitionPayload {
    pub reason: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct LoopRow {
    pub id: String,
    pub lifecycle: String,
    pub ownership: String,
    pub priority: i64,
    pub due_at: Option<String>,
    pub version: i64,
    pub payload_nonce: Vec<u8>,
    pub payload_ciphertext: Vec<u8>,
    pub review_state: String,
    pub scheduled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLoop {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub owner: String,
    pub status: String,
    pub lifecycle: String,
    pub ownership: String,
    pub review_state: String,
    pub scheduled: bool,
    pub priority: i64,
    pub due_at: Option<String>,
    pub version: i64,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CalendarBlock {
    pub id: String,
    pub title: String,
    pub start_at: String,
    pub end_at: String,
    pub kind: String,
    pub color: String,
    pub origin: String,
    pub external_id: Option<String>,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub today: String,
    pub now: String,
    pub briefing: String,
    pub open_loops: Vec<OpenLoop>,
    pub calendar_blocks: Vec<CalendarBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLoopStatusInput {
    pub id: String,
    pub status: String,
    pub expected_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCalendarBlockInput {
    pub title: String,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleConnectorStatus {
    pub state: String,
    pub account_email: Option<String>,
    pub last_sync_at: Option<String>,
    pub next_sync_at: Option<String>,
    pub gmail_message_count: i64,
    pub calendar_event_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleSyncSummary {
    pub gmail_message_count: i64,
    pub calendar_event_count: i64,
    pub completed_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CalendarWhen {
    Timed {
        start_at: String,
        end_at: String,
        time_zone: String,
    },
    AllDay {
        start_date: String,
        end_date: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventInput {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    pub when: CalendarWhen,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub recurrence: Vec<String>,
    #[serde(default = "default_send_updates")]
    pub send_updates: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub when: Option<CalendarWhen>,
    #[serde(default)]
    pub attendees: Option<Vec<String>>,
    #[serde(default)]
    pub recurrence: Option<Vec<String>>,
    #[serde(default = "default_send_updates")]
    pub send_updates: String,
}

impl Default for CalendarEventPatch {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            location: None,
            when: None,
            attendees: None,
            recurrence: None,
            send_updates: default_send_updates(),
        }
    }
}

fn default_send_updates() -> String {
    "all".to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CalendarMutationInput {
    Create {
        operation_id: String,
        event: CalendarEventInput,
    },
    Update {
        operation_id: String,
        event_id: String,
        expected_etag: String,
        patch: CalendarEventPatch,
    },
    Delete {
        operation_id: String,
        event_id: String,
        expected_etag: String,
        #[serde(default = "default_send_updates")]
        send_updates: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMutationResult {
    pub operation_id: String,
    pub event: Option<CalendarBlock>,
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn calendar_mutation_accepts_frontend_camel_case_variant_fields() {
        let input: CalendarMutationInput = serde_json::from_value(json!({
            "action": "create",
            "operationId": "op-1",
            "event": {
                "title": "Kyra QA Sync Test",
                "when": {
                    "kind": "timed",
                    "startAt": "2026-08-22T23:30:00+05:30",
                    "endAt": "2026-08-22T23:45:00+05:30",
                    "timeZone": "Asia/Kolkata"
                },
                "attendees": [],
                "recurrence": [],
                "sendUpdates": "none"
            }
        }))
        .unwrap();

        match input {
            CalendarMutationInput::Create {
                operation_id,
                event:
                    CalendarEventInput {
                        when:
                            CalendarWhen::Timed {
                                start_at,
                                end_at,
                                time_zone,
                            },
                        ..
                    },
            } => {
                assert_eq!(operation_id, "op-1");
                assert_eq!(start_at, "2026-08-22T23:30:00+05:30");
                assert_eq!(end_at, "2026-08-22T23:45:00+05:30");
                assert_eq!(time_zone, "Asia/Kolkata");
            }
            _ => panic!("expected a timed create mutation"),
        }
    }
}
