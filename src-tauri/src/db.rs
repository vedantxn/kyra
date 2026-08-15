use std::fs;

use chrono::{Duration, Local, TimeZone};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use tauri::{AppHandle, Manager};

pub async fn initialize(app: &AppHandle) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let app_data = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data)?;
    let options = SqliteConnectOptions::new()
        .filename(app_data.join("kyra-v1.sqlite3"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    seed_if_empty(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
pub async fn memory_pool() -> SqlitePool {
    let options = <SqliteConnectOptions as std::str::FromStr>::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    seed_if_empty(&pool).await.unwrap();
    pool
}

async fn seed_if_empty(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM open_loops")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let now = Local::now();
    let today = now.date_naive();
    let at = |hour: u32, minute: u32| {
        Local
            .from_local_datetime(&today.and_hms_opt(hour, minute, 0).unwrap())
            .single()
            .unwrap()
            .to_rfc3339()
    };
    let yesterday = (now - Duration::days(1)).to_rfc3339();
    let created = now.to_rfc3339();
    let mut transaction = pool.begin().await?;

    let loops = [
        (
            "waiting-manish",
            "Waiting on Manish for the video edits",
            "You followed up, and Manish said his editor started and would send a few by morning.",
            "them",
            "waiting",
            95_i64,
            Some(at(9, 0)),
        ),
        (
            "waiting-ayush",
            "Waiting on Ayush for the write-up",
            "Ayush promised the write-up for today, but it has not arrived yet.",
            "them",
            "waiting",
            90,
            Some(at(10, 0)),
        ),
        (
            "mail-receipt",
            "Mail the signed form and send the receipt",
            "You said you would mail the form and keep the receipt as proof.",
            "me",
            "open",
            86,
            Some(at(17, 0)),
        ),
        (
            "update-rc",
            "Update RC on how the pitch went",
            "RC asked how it went; you still have not shared the outcome.",
            "me",
            "open",
            78,
            None,
        ),
    ];
    for (id, title, summary, owner, status, priority, due_at) in loops {
        sqlx::query("INSERT INTO open_loops (id, title, summary, owner, status, priority, due_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(id).bind(title).bind(summary).bind(owner).bind(status).bind(priority).bind(due_at).bind(&created).bind(&created)
            .execute(&mut *transaction).await?;
    }

    let evidence = [
        (
            "e1",
            "waiting-manish",
            "fixture_message",
            "Message with Manish",
            "My editor just started. Will send a few by morning.",
        ),
        (
            "e2",
            "waiting-ayush",
            "fixture_message",
            "Message with Ayush",
            "I'll send it tomorrow and add the new material.",
        ),
        (
            "e3",
            "mail-receipt",
            "gmail",
            "Email from Phalanshu",
            "Please keep the USPS receipt as proof.",
        ),
        (
            "e4",
            "update-rc",
            "fixture_message",
            "Message with RC",
            "How did the pitch go?",
        ),
    ];
    for (id, loop_id, kind, label, excerpt) in evidence {
        sqlx::query("INSERT INTO evidence (id, loop_id, source_kind, source_label, excerpt, occurred_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id).bind(loop_id).bind(kind).bind(label).bind(excerpt).bind(&yesterday)
            .execute(&mut *transaction).await?;
    }

    let blocks = [
        (
            "sleep",
            "Night time",
            at(1, 0),
            at(8, 0),
            "routine",
            "#a8c7ee",
        ),
        ("gym", "Gym", at(8, 30), at(9, 30), "execution", "#7bcaa2"),
        (
            "meeting",
            "Kyra product review",
            at(10, 0),
            at(11, 0),
            "meeting",
            "#8eb8ec",
        ),
        (
            "deep-work",
            "Build V1 vertical slice",
            at(13, 0),
            at(16, 0),
            "execution",
            "#86c998",
        ),
    ];
    for (id, title, start_at, end_at, kind, color) in blocks {
        sqlx::query("INSERT INTO calendar_blocks (id, title, start_at, end_at, kind, color, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(id).bind(title).bind(start_at).bind(end_at).bind(kind).bind(color).bind(&created)
            .execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}
