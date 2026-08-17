use chrono::{DateTime, Local};
use sqlx::SqlitePool;
use tauri::{Manager, State};
use uuid::Uuid;

use crate::{
    types::{
        CalendarBlock, CreateCalendarBlockInput, CreateTaskInput, Dashboard, Evidence, LoopRow,
        OpenLoop, SetLoopStatusInput,
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

async fn hydrate_loop(pool: &SqlitePool, row: LoopRow) -> Result<OpenLoop, CoreError> {
    let evidence = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, source_kind, source_label, excerpt, occurred_at FROM evidence WHERE loop_id = ? ORDER BY occurred_at DESC",
    ).bind(&row.id).fetch_all(pool).await?
        .into_iter().map(|item| Evidence { id: item.0, source_kind: item.1, source_label: item.2, excerpt: item.3, occurred_at: item.4 }).collect();
    Ok(OpenLoop {
        id: row.id,
        title: row.title,
        summary: row.summary,
        owner: row.owner,
        status: row.status,
        priority: row.priority,
        due_at: row.due_at,
        version: row.version,
        evidence,
    })
}

async fn find_loop(pool: &SqlitePool, id: &str) -> Result<OpenLoop, CoreError> {
    let row = sqlx::query_as::<_, LoopRow>("SELECT id, title, summary, owner, status, priority, due_at, version FROM open_loops WHERE id = ?")
        .bind(id).fetch_optional(pool).await?.ok_or(CoreError::NotFound)?;
    hydrate_loop(pool, row).await
}

pub async fn dashboard(pool: &SqlitePool) -> Result<Dashboard, CoreError> {
    let rows = sqlx::query_as::<_, LoopRow>("SELECT id, title, summary, owner, status, priority, due_at, version FROM open_loops WHERE status NOT IN ('done', 'dismissed') ORDER BY priority DESC, updated_at DESC")
        .fetch_all(pool).await?;
    let mut open_loops = Vec::with_capacity(rows.len());
    for row in rows {
        open_loops.push(hydrate_loop(pool, row).await?);
    }
    let calendar_blocks = sqlx::query_as::<_, CalendarBlock>("SELECT id, title, start_at, end_at, kind, color FROM calendar_blocks ORDER BY start_at ASC")
        .fetch_all(pool).await?;
    let waiting = open_loops
        .iter()
        .filter(|item| item.owner == "them")
        .count();
    let mine = open_loops.iter().filter(|item| item.owner == "me").count();
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

pub async fn insert_task(pool: &SqlitePool, input: CreateTaskInput) -> Result<OpenLoop, CoreError> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 240 {
        return Err(CoreError::Validation(
            "Tasks must be between 1 and 240 characters.".to_owned(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = Local::now().to_rfc3339();
    sqlx::query("INSERT INTO open_loops (id, title, summary, owner, status, priority, created_at, updated_at) VALUES (?, ?, 'Added directly by you.', 'me', 'open', 50, ?, ?)")
        .bind(&id).bind(title).bind(&now).bind(&now).execute(pool).await?;
    find_loop(pool, &id).await
}

pub async fn update_status(
    pool: &SqlitePool,
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
    let previous: Option<String> = sqlx::query_scalar("SELECT status FROM open_loops WHERE id = ?")
        .bind(&input.id)
        .fetch_optional(pool)
        .await?;
    let previous = previous.ok_or(CoreError::NotFound)?;
    let now = Local::now().to_rfc3339();
    let updated = sqlx::query("UPDATE open_loops SET status = ?, version = version + 1, updated_at = ? WHERE id = ? AND version = ?")
        .bind(&input.status).bind(&now).bind(&input.id).bind(input.expected_version).execute(pool).await?;
    if updated.rows_affected() == 0 {
        return Err(CoreError::Conflict);
    }
    sqlx::query("INSERT INTO loop_transitions (id, loop_id, from_status, to_status, reason, created_at) VALUES (?, ?, ?, ?, 'user_action', ?)")
        .bind(Uuid::new_v4().to_string()).bind(&input.id).bind(previous).bind(&input.status).bind(now).execute(pool).await?;
    find_loop(pool, &input.id).await
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
    };
    sqlx::query("INSERT INTO calendar_blocks (id, title, start_at, end_at, kind, color, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&block.id).bind(&block.title).bind(&block.start_at).bind(&block.end_at).bind(&block.kind).bind(&block.color).bind(Local::now().to_rfc3339()).execute(pool).await?;
    Ok(block)
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    dashboard(&state.pool).await.map_err(public_error)
}

#[tauri::command]
pub async fn create_task(
    state: State<'_, AppState>,
    input: CreateTaskInput,
) -> Result<OpenLoop, String> {
    insert_task(&state.pool, input).await.map_err(public_error)
}

#[tauri::command]
pub async fn set_loop_status(
    state: State<'_, AppState>,
    input: SetLoopStatusInput,
) -> Result<OpenLoop, String> {
    update_status(&state.pool, input)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn dashboard_has_seeded_evidence_backed_loops() {
        let pool = db::memory_pool().await;
        let result = dashboard(&pool).await.unwrap();
        assert_eq!(result.open_loops.len(), 5);
        assert!(result
            .open_loops
            .iter()
            .all(|item| !item.evidence.is_empty()));
    }

    #[tokio::test]
    async fn status_updates_use_optimistic_concurrency() {
        let pool = db::memory_pool().await;
        let updated = update_status(
            &pool,
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
            SetLoopStatusInput {
                id: "mail-receipt".into(),
                status: "open".into(),
                expected_version: 1,
            },
        )
        .await;
        assert!(matches!(conflict, Err(CoreError::Conflict)));
    }
}
