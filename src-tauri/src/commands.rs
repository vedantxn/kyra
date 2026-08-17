use chrono::{DateTime, Local};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{
    ai::types::{
        ActivationReport, AiActivityItem, AiCommandInput, AiCommandResult, AiEngineStatus,
        AiProvider, AiReviewItem, OllamaModel, ResolveAiReviewInput, RevertAiActionResult,
        SaveAiProviderConfigInput,
    },
    types::{
        CalendarBlock, CalendarMutationInput, CalendarMutationResult, CreateCalendarBlockInput,
        CreateTaskInput, Dashboard, Evidence, GoogleConnectorStatus, GoogleSyncSummary,
        LoopPayload, LoopRow, OpenLoop, SetLoopStatusInput, TransitionPayload,
    },
    AppState,
};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Validation(String),
    #[error("This item changed elsewhere. Refresh before trying again.")]
    Conflict,
    #[error("Open loop not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn public_error(error: CoreError) -> String {
    match error {
        CoreError::Database(_) => "Kyra could not update its local database.".to_owned(),
        other => other.to_string(),
    }
}

impl From<crate::crypto::CryptoError> for CoreError {
    fn from(_: crate::crypto::CryptoError) -> Self {
        Self::Validation("Kyra could not decrypt its local data.".to_owned())
    }
}

fn hydrate_loop(
    cipher: &crate::crypto::LocalCipher,
    row: LoopRow,
    evidence: Vec<Evidence>,
) -> Result<OpenLoop, CoreError> {
    let payload: LoopPayload = cipher.decrypt(&row.payload_nonce, &row.payload_ciphertext)?;
    let status = match row.lifecycle.as_str() {
        "resolved" => "done",
        "dismissed" => "dismissed",
        _ if row.ownership == "other" => "waiting",
        _ => "open",
    }
    .to_owned();
    let owner = if row.ownership == "other" {
        "them"
    } else {
        "me"
    }
    .to_owned();
    Ok(OpenLoop {
        id: row.id,
        title: payload.title,
        summary: payload.summary,
        owner,
        status,
        lifecycle: row.lifecycle,
        ownership: row.ownership,
        review_state: row.review_state,
        scheduled: row.scheduled,
        priority: row.priority,
        due_at: row.due_at,
        version: row.version,
        evidence,
    })
}

async fn find_loop(
    pool: &SqlitePool,
    cipher: &crate::crypto::LocalCipher,
    id: &str,
) -> Result<OpenLoop, CoreError> {
    let row = sqlx::query_as::<_, LoopRow>("SELECT id, lifecycle, ownership, priority, due_at, version, payload_nonce, payload_ciphertext, CASE WHEN EXISTS (SELECT 1 FROM ai_reviews r WHERE r.target_loop_id = open_loops.id AND r.status = 'pending') THEN 'needs_review' ELSE 'none' END AS review_state, EXISTS (SELECT 1 FROM loop_calendar_links l WHERE l.loop_id = open_loops.id) AS scheduled FROM open_loops WHERE id = ?")
        .bind(id).fetch_optional(pool).await?.ok_or(CoreError::NotFound)?;
    let evidence = load_evidence(pool, cipher, Some(id), false).await?;
    hydrate_loop(
        cipher,
        row,
        evidence.into_iter().map(|(_, item)| item).collect(),
    )
}

async fn load_evidence(
    pool: &SqlitePool,
    cipher: &crate::crypto::LocalCipher,
    loop_id: Option<&str>,
    hide_demo: bool,
) -> Result<Vec<(String, Evidence)>, CoreError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Vec<u8>, Vec<u8>)>(
        "SELECT e.loop_id, e.id, e.source_kind, e.occurred_at, e.payload_nonce, e.payload_ciphertext FROM evidence e JOIN open_loops l ON l.id = e.loop_id WHERE (? IS NULL OR e.loop_id = ?) AND (? = 0 OR l.origin != 'demo') ORDER BY e.occurred_at DESC",
    )
    .bind(loop_id)
    .bind(loop_id)
    .bind(hide_demo)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(loop_id, id, source_kind, occurred_at, nonce, ciphertext)| {
                let payload: crate::types::EvidencePayload = cipher.decrypt(&nonce, &ciphertext)?;
                Ok((
                    loop_id,
                    Evidence {
                        id,
                        source_kind,
                        source_label: payload.source_label,
                        excerpt: payload.excerpt,
                        occurred_at,
                    },
                ))
            },
        )
        .collect()
}

pub async fn dashboard(
    pool: &SqlitePool,
    cipher: &crate::crypto::LocalCipher,
    hide_demo: bool,
    mut provider_calendar_blocks: Vec<CalendarBlock>,
) -> Result<Dashboard, CoreError> {
    let rows = sqlx::query_as::<_, LoopRow>("SELECT id, lifecycle, ownership, priority, due_at, version, payload_nonce, payload_ciphertext, CASE WHEN EXISTS (SELECT 1 FROM ai_reviews r WHERE r.target_loop_id = open_loops.id AND r.status = 'pending') THEN 'needs_review' ELSE 'none' END AS review_state, EXISTS (SELECT 1 FROM loop_calendar_links l WHERE l.loop_id = open_loops.id) AS scheduled FROM open_loops WHERE lifecycle = 'active' AND (? = 0 OR origin != 'demo') ORDER BY priority DESC, updated_at DESC")
        .bind(hide_demo)
        .fetch_all(pool).await?;
    let mut evidence_by_loop: HashMap<String, Vec<Evidence>> = HashMap::new();
    for (loop_id, evidence) in load_evidence(pool, cipher, None, hide_demo).await? {
        evidence_by_loop.entry(loop_id).or_default().push(evidence);
    }
    let mut open_loops = Vec::with_capacity(rows.len());
    for row in rows {
        let evidence = evidence_by_loop.remove(&row.id).unwrap_or_default();
        open_loops.push(hydrate_loop(cipher, row, evidence)?);
    }
    let mut calendar_blocks = sqlx::query_as::<_, CalendarBlock>("SELECT id, title, start_at, end_at, kind, color, origin, external_id, etag FROM calendar_blocks WHERE (? = 0 OR origin != 'demo') ORDER BY start_at ASC")
        .bind(hide_demo)
        .fetch_all(pool).await?;
    calendar_blocks.append(&mut provider_calendar_blocks);
    calendar_blocks.sort_by(|left, right| left.start_at.cmp(&right.start_at));
    let waiting = open_loops
        .iter()
        .filter(|item| item.ownership == "other")
        .count();
    let mine = open_loops
        .iter()
        .filter(|item| matches!(item.ownership.as_str(), "me" | "shared"))
        .count();
    let has_reference_context = open_loops.iter().any(|item| item.id == "waiting-manish")
        && open_loops.iter().any(|item| item.id == "waiting-ayush");
    let briefing = if has_reference_context {
        "Manish and Ayush still owe you the video edits and the write-up, while the 83(b) mailing to Phalanshu and RC's update on the pitch are still on you.".to_owned()
    } else {
        match (waiting, mine) {
            (0, 0) => "Nothing is slipping through. Your day is clear.".to_owned(),
            (0, mine) => format!("{mine} open loops are still on you. Protect time for the highest-priority one."),
            (waiting, 0) => format!("{waiting} open loops are waiting on other people. You have no active follow-ups on your side."),
            (waiting, mine) => format!("{waiting} open loops are waiting on other people, while {mine} are still on you. Start with the highest-priority commitment."),
        }
    };
    let now = Local::now();
    Ok(Dashboard {
        today: now.date_naive().to_string(),
        now: now.to_rfc3339(),
        briefing,
        open_loops,
        calendar_blocks,
    })
}

pub async fn insert_task(
    pool: &SqlitePool,
    cipher: &crate::crypto::LocalCipher,
    input: CreateTaskInput,
) -> Result<OpenLoop, CoreError> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 240 {
        return Err(CoreError::Validation(
            "Tasks must be between 1 and 240 characters.".to_owned(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = Local::now().to_rfc3339();
    let payload = LoopPayload {
        title: title.to_owned(),
        summary: "Added directly by you.".to_owned(),
    };
    let (nonce, ciphertext) = cipher.encrypt(&payload)?;
    let (title_nonce, title_ciphertext) = cipher.encrypt(&payload.title)?;
    let mut transaction = pool.begin().await?;
    sqlx::query("INSERT INTO open_loops (id, title, summary, owner, status, priority, lifecycle, ownership, payload_nonce, payload_ciphertext, payload_migrated, created_at, updated_at) VALUES (?, 'Encrypted', '', 'me', 'open', 50, 'active', 'me', ?, ?, 1, ?, ?)")
        .bind(&id).bind(nonce).bind(ciphertext).bind(&now).bind(&now).execute(&mut *transaction).await?;
    sqlx::query("INSERT INTO loop_derivations (id, loop_id, field_name, source_type, active, value_hash, payload_nonce, payload_ciphertext, created_at) VALUES (?, ?, 'title', 'user', 1, ?, ?, ?, ?)")
        .bind(Uuid::new_v4().to_string()).bind(&id).bind(cipher.pseudonymous_id("loop:title", &payload.title)).bind(title_nonce).bind(title_ciphertext).bind(&now).execute(&mut *transaction).await?;
    transaction.commit().await?;
    find_loop(pool, cipher, &id).await
}

pub async fn update_status(
    pool: &SqlitePool,
    cipher: &crate::crypto::LocalCipher,
    input: SetLoopStatusInput,
) -> Result<OpenLoop, CoreError> {
    if !matches!(
        input.status.as_str(),
        "open" | "waiting" | "done" | "dismissed"
    ) {
        return Err(CoreError::Validation(
            "Unknown open-loop status.".to_owned(),
        ));
    }
    let lifecycle = match input.status.as_str() {
        "open" | "waiting" => "active",
        "done" => "resolved",
        "dismissed" => "dismissed",
        _ => unreachable!(),
    };
    let ownership = if input.status == "waiting" {
        "other"
    } else {
        "me"
    };
    let previous: Option<String> =
        sqlx::query_scalar("SELECT lifecycle FROM open_loops WHERE id = ?")
            .bind(&input.id)
            .fetch_optional(pool)
            .await?;
    let previous = previous.ok_or(CoreError::NotFound)?;
    let now = Local::now().to_rfc3339();
    let updated = sqlx::query("UPDATE open_loops SET status = ?, owner = ?, lifecycle = ?, ownership = ?, version = version + 1, updated_at = ? WHERE id = ? AND version = ?")
        .bind(&input.status).bind(if ownership == "other" { "them" } else { "me" }).bind(lifecycle).bind(ownership).bind(&now).bind(&input.id).bind(input.expected_version).execute(pool).await?;
    if updated.rows_affected() == 0 {
        return Err(CoreError::Conflict);
    }
    let (reason_nonce, reason_ciphertext) = cipher.encrypt(&TransitionPayload {
        reason: "user_action".to_owned(),
    })?;
    sqlx::query("INSERT INTO loop_transitions (id, loop_id, from_status, to_status, reason, payload_nonce, payload_ciphertext, payload_migrated, created_at) VALUES (?, ?, ?, ?, 'encrypted', ?, ?, 1, ?)")
        .bind(Uuid::new_v4().to_string()).bind(&input.id).bind(previous).bind(&input.status).bind(reason_nonce).bind(reason_ciphertext).bind(now).execute(pool).await?;
    find_loop(pool, cipher, &input.id).await
}

pub async fn insert_calendar_block(
    pool: &SqlitePool,
    input: CreateCalendarBlockInput,
) -> Result<CalendarBlock, CoreError> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 240 {
        return Err(CoreError::Validation(
            "Calendar blocks must have a title under 240 characters.".to_owned(),
        ));
    }
    let start = DateTime::parse_from_rfc3339(&input.start_at)
        .map_err(|_| CoreError::Validation("Start time is not valid.".to_owned()))?;
    let end = DateTime::parse_from_rfc3339(&input.end_at)
        .map_err(|_| CoreError::Validation("End time is not valid.".to_owned()))?;
    if end <= start || end - start > chrono::Duration::hours(24) {
        return Err(CoreError::Validation(
            "Calendar blocks must end after they start and be under 24 hours.".to_owned(),
        ));
    }
    let block = CalendarBlock {
        id: Uuid::new_v4().to_string(),
        title: title.to_owned(),
        start_at: input.start_at,
        end_at: input.end_at,
        kind: "execution".to_owned(),
        color: "#8ca481".to_owned(),
        origin: "local".to_owned(),
        external_id: None,
        etag: None,
    };
    sqlx::query("INSERT INTO calendar_blocks (id, title, start_at, end_at, kind, color, origin, created_at) VALUES (?, ?, ?, ?, ?, ?, 'local', ?)")
        .bind(&block.id).bind(&block.title).bind(&block.start_at).bind(&block.end_at).bind(&block.kind).bind(&block.color).bind(Local::now().to_rfc3339()).execute(pool).await?;
    Ok(block)
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let connector_status = state
        .google
        .status()
        .await
        .map_err(|error| error.public_message())?;
    let connected = connector_status.state != "disconnected";
    let provider_blocks = if connected {
        state.google.calendar_blocks().await.unwrap_or_default()
    } else {
        Vec::new()
    };
    dashboard(&state.pool, &state.cipher, connected, provider_blocks)
        .await
        .map_err(public_error)
}

#[tauri::command]
pub async fn create_task(
    state: State<'_, AppState>,
    input: CreateTaskInput,
) -> Result<OpenLoop, String> {
    insert_task(&state.pool, &state.cipher, input)
        .await
        .map_err(public_error)
}

#[tauri::command]
pub async fn set_loop_status(
    state: State<'_, AppState>,
    input: SetLoopStatusInput,
) -> Result<OpenLoop, String> {
    update_status(&state.pool, &state.cipher, input)
        .await
        .map_err(public_error)
}

#[tauri::command]
pub async fn create_calendar_block(
    state: State<'_, AppState>,
    input: CreateCalendarBlockInput,
) -> Result<CalendarBlock, String> {
    insert_calendar_block(&state.pool, input)
        .await
        .map_err(public_error)
}

#[tauri::command]
pub fn hide_overlay(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Kyra's main window is unavailable.".to_owned())?
        .hide()
        .map_err(|_| "Kyra could not hide its window.".to_owned())
}

#[tauri::command]
pub async fn get_google_connector_status(
    state: State<'_, AppState>,
) -> Result<GoogleConnectorStatus, String> {
    state
        .google
        .status()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn connect_google(state: State<'_, AppState>) -> Result<GoogleConnectorStatus, String> {
    state
        .google
        .connect()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn disconnect_google(
    state: State<'_, AppState>,
) -> Result<GoogleConnectorStatus, String> {
    state
        .google
        .disconnect()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn sync_google_now(state: State<'_, AppState>) -> Result<GoogleSyncSummary, String> {
    state
        .google
        .sync_now()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn mutate_google_calendar(
    state: State<'_, AppState>,
    input: CalendarMutationInput,
) -> Result<CalendarMutationResult, String> {
    state
        .google
        .mutate_calendar(input)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn get_ai_engine_status(state: State<'_, AppState>) -> Result<AiEngineStatus, String> {
    state
        .ai
        .status()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn save_ai_provider_config(
    state: State<'_, AppState>,
    input: SaveAiProviderConfigInput,
) -> Result<AiEngineStatus, String> {
    state
        .ai
        .save_config(input)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn clear_ai_provider(
    state: State<'_, AppState>,
    provider: AiProvider,
) -> Result<AiEngineStatus, String> {
    state
        .ai
        .clear_provider(provider)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn list_ollama_models(
    state: State<'_, AppState>,
    base_url: Option<String>,
) -> Result<Vec<OllamaModel>, String> {
    state
        .ai
        .list_ollama_models(base_url)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn test_ai_provider(state: State<'_, AppState>) -> Result<ActivationReport, String> {
    state
        .ai
        .activate()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn run_ai_now(state: State<'_, AppState>) -> Result<AiEngineStatus, String> {
    state
        .ai
        .run_now()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn execute_ai_command(
    state: State<'_, AppState>,
    input: AiCommandInput,
) -> Result<AiCommandResult, String> {
    state
        .ai
        .execute_command(input)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn list_ai_reviews(state: State<'_, AppState>) -> Result<Vec<AiReviewItem>, String> {
    state
        .ai
        .reviews()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn resolve_ai_review(
    state: State<'_, AppState>,
    input: ResolveAiReviewInput,
) -> Result<Vec<String>, String> {
    state
        .ai
        .resolve_review(&input.review_id, &input.decision)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn list_ai_activity(state: State<'_, AppState>) -> Result<Vec<AiActivityItem>, String> {
    state
        .ai
        .activity()
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn retry_ai_job(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<AiEngineStatus, String> {
    state
        .ai
        .retry_job(&job_id)
        .await
        .map_err(|error| error.public_message())
}

#[tauri::command]
pub async fn revert_ai_action(
    state: State<'_, AppState>,
    action_id: String,
) -> Result<RevertAiActionResult, String> {
    state
        .ai
        .revert_action(&action_id)
        .await
        .map_err(|error| error.public_message())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn dashboard_has_seeded_evidence_backed_loops() {
        let (pool, cipher) = db::memory_secure().await;
        let result = dashboard(&pool, &cipher, false, Vec::new()).await.unwrap();
        assert_eq!(result.open_loops.len(), 5);
        assert!(result
            .open_loops
            .iter()
            .all(|item| !item.evidence.is_empty()));
    }

    #[tokio::test]
    async fn status_updates_use_optimistic_concurrency() {
        let (pool, cipher) = db::memory_secure().await;
        let updated = update_status(
            &pool,
            &cipher,
            SetLoopStatusInput {
                id: "mail-receipt".into(),
                status: "done".into(),
                expected_version: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.status, "done");
        let conflict = update_status(
            &pool,
            &cipher,
            SetLoopStatusInput {
                id: "mail-receipt".into(),
                status: "open".into(),
                expected_version: 1,
            },
        )
        .await;
        assert!(matches!(conflict, Err(CoreError::Conflict)));
    }

    #[tokio::test]
    async fn connected_dashboard_hides_demo_but_preserves_local_items() {
        let (pool, cipher) = db::memory_secure().await;
        let local = insert_task(
            &pool,
            &cipher,
            CreateTaskInput {
                title: "Keep this local task".into(),
            },
        )
        .await
        .unwrap();
        let google_block = CalendarBlock {
            id: "google:event-1".into(),
            title: "Real event".into(),
            start_at: "2026-08-17T10:00:00Z".into(),
            end_at: "2026-08-17T11:00:00Z".into(),
            kind: "meeting".into(),
            color: "#b7b9b2".into(),
            origin: "google".into(),
            external_id: Some("event-1".into()),
            etag: Some("v1".into()),
        };
        let result = dashboard(&pool, &cipher, true, vec![google_block])
            .await
            .unwrap();
        assert_eq!(result.open_loops.len(), 1);
        assert_eq!(result.open_loops[0].id, local.id);
        assert_eq!(result.calendar_blocks.len(), 1);
        assert_eq!(result.calendar_blocks[0].origin, "google");
    }
}
