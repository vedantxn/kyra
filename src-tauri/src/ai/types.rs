use serde::{Deserialize, Serialize};

pub const INTENT_SCHEMA_VERSION: &str = "kyra.intent.v1";
pub const PROMPT_VERSION: &str = "kyra.prompt.v1";
pub const POLICY_VERSION: &str = "kyra.policy.v1";
pub const REDACTION_VERSION: &str = "kyra.redaction.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Openai,
    Anthropic,
    Ollama,
}

impl AiProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Ollama => "ollama",
        }
    }

    pub fn is_cloud(self) -> bool {
        !matches!(self, Self::Ollama)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAiProviderConfigInput {
    pub provider: AiProvider,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEngineStatus {
    pub state: String,
    pub provider: Option<AiProvider>,
    pub requested_model: Option<String>,
    pub activated_model: Option<String>,
    pub activation_expires_at: Option<String>,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub queued_jobs: i64,
    pub running_jobs: i64,
    pub failed_jobs: i64,
    pub review_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiCommandInput {
    pub text: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiCommandResult {
    Executed {
        action_ids: Vec<String>,
    },
    ClarificationRequired {
        session_id: String,
        question: String,
        expires_at: String,
    },
    ReviewCreated {
        review_id: String,
    },
    NoAction {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub irreversible_effects: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveAiReviewInput {
    pub review_id: String,
    pub decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiActivityItem {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub can_revert: bool,
    pub compensation: bool,
    pub irreversible_effects: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertAiActionResult {
    pub action_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceReference {
    pub source_revision_id: String,
    pub document_hash: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub quote_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentAction {
    TaskCreate,
    TaskUpdate,
    ResolutionSuggest,
    CalendarCreate,
    CalendarReschedule,
    CalendarCancel,
    CalendarDelete,
    BriefingOrder,
    NoAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalendarProposal {
    pub event_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub all_day_start: Option<String>,
    pub all_day_end: Option<String>,
    pub time_zone: Option<String>,
    #[serde(default)]
    pub attendee_person_ids: Vec<String>,
    #[serde(default)]
    pub recurrence: Vec<String>,
    pub expected_etag: Option<String>,
    pub send_updates: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentProposal {
    pub proposal_id: String,
    pub action: IntentAction,
    pub target_loop_id: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub ownership: Option<String>,
    pub due_at: Option<String>,
    pub calendar: Option<CalendarProposal>,
    #[serde(default)]
    pub person_ids: Vec<String>,
    #[serde(default)]
    pub fact_ids: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceReference>,
    pub confidence: f32,
    pub ambiguity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentEnvelope {
    pub schema_version: String,
    pub activation_fingerprint: String,
    pub source_document_hash: String,
    #[serde(default)]
    pub proposals: Vec<IntentProposal>,
}

#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub system_prompt: String,
    pub document: String,
    pub schema: serde_json::Value,
    pub activation_fingerprint: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub input_units: Option<i64>,
    pub output_units: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ProviderInference {
    pub envelope: IntentEnvelope,
    pub resolved_model: String,
    pub usage: ProviderUsage,
    pub latency_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider: AiProvider,
    pub requested_model: String,
    pub resolved_model: String,
    pub model_digest: Option<String>,
    pub latency_ms: i64,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: AiProvider,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaModel {
    pub name: String,
    pub digest: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationReport {
    pub fingerprint: String,
    pub provider: AiProvider,
    pub requested_model: String,
    pub resolved_model: String,
    pub cases_run: usize,
    pub schema_validity: f32,
    pub evidence_validity: f32,
    pub required_action_coverage: f32,
    pub confirmed_meeting_recall: f32,
    pub unauthorized_actions: usize,
    pub ambiguous_calendar_actions: usize,
    pub max_latency_ms: i64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalMessage {
    pub source_revision_id: String,
    pub person_id: String,
    pub occurred_at: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanMap {
    pub source_revision_id: String,
    pub original_start: usize,
    pub original_end: usize,
    pub transformed_start: usize,
    pub transformed_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalDocument {
    pub text: String,
    pub document_hash: String,
    pub truncated: bool,
    pub span_map: Vec<SpanMap>,
    pub source_revision_ids: Vec<String>,
    pub person_ids: Vec<String>,
}
