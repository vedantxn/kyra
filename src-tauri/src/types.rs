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

#[derive(Debug, Clone, FromRow)]
pub struct LoopRow {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub owner: String,
    pub status: String,
    pub priority: i64,
    pub due_at: Option<String>,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLoop {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub owner: String,
    pub status: String,
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
