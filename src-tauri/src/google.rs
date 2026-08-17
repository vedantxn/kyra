use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration as StdDuration,
};

use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use futures::{stream, StreamExt, TryStreamExt};
use keyring::{Entry, Error as KeyringError};
use reqwest::{Client, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use tauri_plugin_opener::open_url;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
    time::{sleep, timeout},
};
use url::Url;
use uuid::Uuid;

use crate::types::{
    CalendarBlock, CalendarEventInput, CalendarEventPatch, CalendarMutationInput,
    CalendarMutationResult, CalendarWhen, GoogleConnectorStatus, GoogleSyncSummary,
};

const GOOGLE_KEYCHAIN_SERVICE: &str = "com.vedant.kyra.google";
const GOOGLE_SCOPES: &str = "openid email https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/calendar.events";
const FIVE_MINUTES: i64 = 5;
const GMAIL_LIMIT: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("Google is not connected.")]
    NotConnected,
    #[error("Add KYRA_GOOGLE_CLIENT_ID to .env.local before connecting Google.")]
    MissingClientId,
    #[error("Google authorization was cancelled.")]
    AuthorizationDenied,
    #[error("The Google authorization response could not be trusted.")]
    InvalidOAuthState,
    #[error("Google authorization timed out. Try connecting again.")]
    OAuthTimeout,
    #[error("Google access expired or was revoked. Reconnect the account.")]
    ReconnectRequired,
    #[error("This event changed in Google. Kyra refreshed the calendar; try again with the new version.")]
    Conflict,
    #[error("A connector operation with this ID already has different content.")]
    OperationMismatch,
    #[error("{0}")]
    Validation(String),
    #[error("Google is temporarily limiting requests. Kyra will retry automatically.")]
    RateLimited,
    #[error("Kyra could not reach Google. Cached data is still available.")]
    Network,
    #[error("Google returned an unexpected response.")]
    Provider,
    #[error("Kyra could not use macOS Keychain.")]
    Keychain,
    #[error("Kyra could not encrypt or decrypt connected data.")]
    Crypto,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl GoogleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConnected => "not_connected",
            Self::MissingClientId => "missing_client_id",
            Self::AuthorizationDenied => "authorization_denied",
            Self::InvalidOAuthState => "invalid_oauth_state",
            Self::OAuthTimeout => "oauth_timeout",
            Self::ReconnectRequired => "reconnect_required",
            Self::Conflict => "calendar_conflict",
            Self::OperationMismatch => "operation_mismatch",
            Self::Validation(_) => "validation",
            Self::RateLimited => "rate_limited",
            Self::Network => "network",
            Self::Provider => "provider",
            Self::Keychain => "keychain",
            Self::Crypto => "crypto",
            Self::Database(_) => "database",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Database(_) => "Kyra could not update its local connector database.".to_owned(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct GoogleEndpoints {
    auth: String,
    token: String,
    userinfo: String,
    gmail: String,
    calendar: String,
}

impl Default for GoogleEndpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
            token: "https://oauth2.googleapis.com/token".to_owned(),
            userinfo: "https://openidconnect.googleapis.com/v1/userinfo".to_owned(),
            gmail: "https://gmail.googleapis.com/gmail/v1".to_owned(),
            calendar: "https://www.googleapis.com/calendar/v3".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
struct AccountRow {
    id: String,
    state: String,
    email_nonce: Vec<u8>,
    email_ciphertext: Vec<u8>,
    gmail_history_id: Option<String>,
    calendar_sync_token: Option<String>,
    calendar_window_anchor: Option<String>,
    last_sync_at: Option<String>,
    next_sync_at: Option<String>,
    last_error_code: Option<String>,
    generation: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    sub: String,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGmailMessage {
    id: String,
    thread_id: String,
    subject: String,
    from: String,
    to: Vec<String>,
    cc: Vec<String>,
    body_text: String,
    snippet: String,
    occurred_at: String,
    labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AiGmailMessage {
    pub source_revision_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub body_text: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AiThreadSource {
    pub account_email: String,
    pub messages: Vec<AiGmailMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCalendarSnapshot {
    pub id: String,
    pub etag: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub all_day: bool,
    pub time_zone: Option<String>,
    pub attendees: Vec<String>,
    pub recurrence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCalendarEvent {
    id: String,
    etag: String,
    status: String,
    title: String,
    description: Option<String>,
    location: Option<String>,
    start_at: String,
    end_at: String,
    all_day: bool,
    time_zone: Option<String>,
    attendees: Vec<String>,
    recurrence: Vec<String>,
    recurring_event_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessageRef {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailMessage {
    id: String,
    thread_id: String,
    #[serde(default)]
    label_ids: Vec<String>,
    #[serde(default)]
    snippet: String,
    internal_date: String,
    payload: GmailPayload,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GmailPayload {
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    headers: Vec<GmailHeader>,
    #[serde(default)]
    body: GmailBody,
    #[serde(default)]
    parts: Vec<GmailPayload>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GmailBody {
    #[serde(default)]
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailListResponse {
    #[serde(default)]
    messages: Vec<GmailMessageRef>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailProfile {
    history_id: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GmailHistoryResponse {
    #[serde(default)]
    history: Vec<GmailHistoryEntry>,
    next_page_token: Option<String>,
    history_id: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GmailHistoryEntry {
    #[serde(default)]
    messages_added: Vec<GmailHistoryMessage>,
    #[serde(default)]
    messages_deleted: Vec<GmailHistoryMessage>,
}

#[derive(Debug, Deserialize)]
struct GmailHistoryMessage {
    message: GmailMessageRef,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GoogleCalendarDateTime {
    #[serde(default)]
    date_time: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    time_zone: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GoogleCalendarAttendee {
    email: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct GoogleCalendarEvent {
    id: String,
    #[serde(default)]
    etag: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    start: GoogleCalendarDateTime,
    #[serde(default)]
    end: GoogleCalendarDateTime,
    #[serde(default)]
    attendees: Vec<GoogleCalendarAttendee>,
    #[serde(default)]
    recurrence: Vec<String>,
    #[serde(default)]
    recurring_event_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListResponse {
    #[serde(default)]
    items: Vec<GoogleCalendarEvent>,
    next_page_token: Option<String>,
    next_sync_token: Option<String>,
}

pub struct GoogleConnector {
    pool: SqlitePool,
    client: Client,
    endpoints: GoogleEndpoints,
    client_id: Option<String>,
    sync_lock: Mutex<()>,
    tokens: Mutex<HashMap<String, CachedToken>>,
}

impl GoogleConnector {
    pub fn new(pool: SqlitePool) -> Arc<Self> {
        let _ = dotenvy::from_filename(".env.local");
        let client_id = std::env::var("KYRA_GOOGLE_CLIENT_ID")
            .ok()
            .or_else(|| option_env!("KYRA_GOOGLE_CLIENT_ID").map(str::to_owned))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        Arc::new(Self {
            pool,
            client: Client::builder()
                .timeout(StdDuration::from_secs(30))
                .user_agent("Kyra/0.1")
                .build()
                .expect("failed to build Google HTTP client"),
            endpoints: GoogleEndpoints::default(),
            client_id,
            sync_lock: Mutex::new(()),
            tokens: Mutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    fn with_endpoints(pool: SqlitePool, endpoints: GoogleEndpoints) -> Arc<Self> {
        Arc::new(Self {
            pool,
            client: Client::builder()
                .timeout(StdDuration::from_secs(2))
                .build()
                .unwrap(),
            endpoints,
            client_id: Some("test-client".to_owned()),
            sync_lock: Mutex::new(()),
            tokens: Mutex::new(HashMap::new()),
        })
    }

    async fn account(&self) -> Result<AccountRow, GoogleError> {
        sqlx::query_as::<_, AccountRow>(
            "SELECT id, state, email_nonce, email_ciphertext, gmail_history_id, calendar_sync_token, calendar_window_anchor, last_sync_at, next_sync_at, last_error_code, generation FROM connector_accounts WHERE provider = 'google' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(GoogleError::NotConnected)
    }

    pub async fn status(&self) -> Result<GoogleConnectorStatus, GoogleError> {
        let account = match self.account().await {
            Ok(account) => account,
            Err(GoogleError::NotConnected) => {
                return Ok(GoogleConnectorStatus {
                    state: "disconnected".to_owned(),
                    account_email: None,
                    last_sync_at: None,
                    next_sync_at: None,
                    gmail_message_count: 0,
                    calendar_event_count: 0,
                    last_error: None,
                });
            }
            Err(error) => return Err(error),
        };
        let key = load_data_key(&account.id);
        let account_email = key.as_ref().ok().and_then(|key| {
            decrypt_value::<String>(key, &account.email_nonce, &account.email_ciphertext).ok()
        });
        let (gmail_message_count, calendar_event_count) = self.item_counts(&account.id).await?;
        let key_error = key.err().map(|error| error.public_message());
        Ok(GoogleConnectorStatus {
            state: if key_error.is_some() {
                "error".to_owned()
            } else {
                account.state
            },
            account_email,
            last_sync_at: account.last_sync_at,
            next_sync_at: account.next_sync_at,
            gmail_message_count,
            calendar_event_count,
            last_error: key_error.or_else(|| account.last_error_code.map(public_error_for_code)),
        })
    }

    async fn item_counts(&self, account_id: &str) -> Result<(i64, i64), GoogleError> {
        let gmail: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_items WHERE account_id = ? AND kind = 'gmail_message'",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;
        let calendar: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND status != 'cancelled'",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;
        Ok((gmail, calendar))
    }

    pub async fn connect(&self) -> Result<GoogleConnectorStatus, GoogleError> {
        let client_id = self.client_id.clone().ok_or(GoogleError::MissingClientId)?;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| GoogleError::Network)?;
        let address = listener.local_addr().map_err(|_| GoogleError::Network)?;
        let redirect_uri = format!("http://127.0.0.1:{}/oauth/callback", address.port());
        let state = random_urlsafe(24);
        let verifier = random_urlsafe(32);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut authorization =
            Url::parse(&self.endpoints.auth).map_err(|_| GoogleError::Provider)?;
        authorization
            .query_pairs_mut()
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", GOOGLE_SCOPES)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        open_url(authorization.as_str(), None::<&str>).map_err(|_| GoogleError::Network)?;

        let callback = timeout(
            StdDuration::from_secs(180),
            receive_oauth_callback(listener),
        )
        .await
        .map_err(|_| GoogleError::OAuthTimeout)??;
        let code = validate_oauth_callback(callback, &state)?;
        let token: TokenResponse = self
            .post_form_json(
                &self.endpoints.token,
                &[
                    ("client_id", client_id.as_str()),
                    ("code", code.as_str()),
                    ("code_verifier", verifier.as_str()),
                    ("grant_type", "authorization_code"),
                    ("redirect_uri", redirect_uri.as_str()),
                ],
            )
            .await?;
        let refresh_token = token.refresh_token.ok_or(GoogleError::ReconnectRequired)?;
        let user: UserInfo = self
            .authorized_get(&self.endpoints.userinfo, &token.access_token)
            .await?
            .json()
            .await
            .map_err(|_| GoogleError::Provider)?;

        if let Ok(previous) = self.account().await {
            self.remove_account(&previous.id).await?;
        }
        let data_key = generate_data_key();
        let (email_nonce, email_ciphertext) = encrypt_value(&data_key, &user.email)?;
        store_refresh_token(&user.sub, &refresh_token)?;
        if let Err(error) = store_data_key(&user.sub, &data_key) {
            let _ = delete_secret(&user.sub, "refresh_token");
            return Err(error);
        }
        let now = Utc::now();
        let insert_result = sqlx::query("INSERT INTO connector_accounts (id, provider, state, email_nonce, email_ciphertext, granted_scopes, next_sync_at, created_at, updated_at) VALUES (?, 'google', 'connected', ?, ?, ?, ?, ?, ?)")
            .bind(&user.sub)
            .bind(email_nonce)
            .bind(email_ciphertext)
            .bind(GOOGLE_SCOPES)
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await;
        if let Err(error) = insert_result {
            let _ = delete_secret(&user.sub, "refresh_token");
            let _ = delete_secret(&user.sub, "data_key");
            return Err(GoogleError::Database(error));
        }
        self.tokens.lock().await.insert(
            user.sub,
            CachedToken {
                access_token: token.access_token,
                expires_at: now + Duration::seconds(token.expires_in.saturating_sub(60)),
            },
        );
        self.sync_now().await?;
        self.status().await
    }

    pub async fn disconnect(&self) -> Result<GoogleConnectorStatus, GoogleError> {
        let _guard = self.sync_lock.lock().await;
        if let Ok(account) = self.account().await {
            self.remove_account(&account.id).await?;
        }
        self.status().await
    }

    async fn remove_account(&self, account_id: &str) -> Result<(), GoogleError> {
        let cleanup_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let mut fence = self.pool.begin().await?;
        sqlx::query("UPDATE connector_accounts SET generation = generation + 1, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(account_id)
            .execute(&mut *fence)
            .await?;
        sqlx::query("UPDATE ai_jobs SET status = 'cancelled', lease_owner = NULL, lease_token = NULL, leased_until = NULL, updated_at = ? WHERE account_id = ? AND status IN ('queued', 'failed')")
            .bind(&now)
            .bind(account_id)
            .execute(&mut *fence)
            .await?;
        fence.commit().await?;

        let drain_deadline = tokio::time::Instant::now() + StdDuration::from_secs(5);
        loop {
            let active: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM ai_jobs WHERE account_id = ? AND status = 'leased'",
            )
            .bind(account_id)
            .fetch_one(&self.pool)
            .await?;
            if active == 0 || tokio::time::Instant::now() >= drain_deadline {
                break;
            }
            sleep(StdDuration::from_millis(100)).await;
        }

        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE ai_jobs SET status = 'cancelled', lease_owner = NULL, lease_token = NULL, leased_until = NULL, updated_at = ? WHERE account_id = ? AND status = 'leased'")
            .bind(&now)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM open_loops WHERE origin = 'google' AND NOT EXISTS (SELECT 1 FROM loop_derivations d WHERE d.loop_id = open_loops.id AND d.source_type IN ('user', 'command') AND d.active = 1)")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM calendar_blocks WHERE origin = 'google'")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("INSERT INTO ai_secret_cleanup (id, service, account, status, created_at, updated_at) VALUES (?, ?, ?, 'pending', ?, ?)")
            .bind(&cleanup_id)
            .bind(GOOGLE_KEYCHAIN_SERVICE)
            .bind(account_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM connector_accounts WHERE id = ?")
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;

        let secrets_removed = delete_secret(account_id, "refresh_token").is_ok()
            && delete_secret(account_id, "data_key").is_ok();
        if secrets_removed {
            sqlx::query(
                "UPDATE ai_secret_cleanup SET status = 'complete', updated_at = ? WHERE id = ?",
            )
            .bind(Utc::now().to_rfc3339())
            .bind(cleanup_id)
            .execute(&self.pool)
            .await?;
        }
        self.tokens.lock().await.remove(account_id);
        Ok(())
    }

    async fn access_token(&self, account: &AccountRow) -> Result<String, GoogleError> {
        if let Some(token) = self.tokens.lock().await.get(&account.id).cloned() {
            if token.expires_at > Utc::now() {
                return Ok(token.access_token);
            }
        }
        let client_id = self.client_id.clone().ok_or(GoogleError::MissingClientId)?;
        let refresh_token = load_refresh_token(&account.id)?;
        let response = self
            .client
            .post(&self.endpoints.token)
            .form(&[
                ("client_id", client_id.as_str()),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        if response.status() == StatusCode::BAD_REQUEST {
            self.set_account_error(&account.id, &GoogleError::ReconnectRequired)
                .await?;
            return Err(GoogleError::ReconnectRequired);
        }
        let response = checked_response(response)?;
        let token: TokenResponse = response.json().await.map_err(|_| GoogleError::Provider)?;
        let expires_at = Utc::now() + Duration::seconds(token.expires_in.saturating_sub(60));
        self.tokens.lock().await.insert(
            account.id.clone(),
            CachedToken {
                access_token: token.access_token.clone(),
                expires_at,
            },
        );
        Ok(token.access_token)
    }

    async fn authorized_get(&self, url: &str, access_token: &str) -> Result<Response, GoogleError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        checked_response(response)
    }

    async fn post_form_json<T: DeserializeOwned>(
        &self,
        url: &str,
        form: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> Result<T, GoogleError> {
        let values: Vec<(&str, &str)> = form
            .iter()
            .map(|(key, value)| (key.as_ref(), value.as_ref()))
            .collect();
        let response = self
            .client
            .post(url)
            .form(&values)
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        checked_response(response)?
            .json()
            .await
            .map_err(|_| GoogleError::Provider)
    }

    async fn set_account_error(
        &self,
        account_id: &str,
        error: &GoogleError,
    ) -> Result<(), GoogleError> {
        let state = if matches!(error, GoogleError::ReconnectRequired) {
            "reconnect_required"
        } else {
            "error"
        };
        let retry_count: i64 =
            sqlx::query_scalar("SELECT retry_count FROM connector_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or(0_i64)
                .saturating_add(1);
        let delay_minutes = (1_i64 << retry_count.min(5)) * FIVE_MINUTES;
        let next_sync = Utc::now() + Duration::minutes(delay_minutes.min(160));
        sqlx::query("UPDATE connector_accounts SET state = ?, retry_count = ?, last_error_code = ?, next_sync_at = ?, updated_at = ? WHERE id = ?")
            .bind(state)
            .bind(retry_count)
            .bind(error.code())
            .bind(next_sync.to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind(account_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn sync_now(&self) -> Result<GoogleSyncSummary, GoogleError> {
        let _guard = self.sync_lock.lock().await;
        self.sync_locked().await
    }

    pub async fn sync_if_due(&self) {
        let account = match self.account().await {
            Ok(account) => account,
            Err(_) => return,
        };
        if account.state == "reconnect_required" {
            return;
        }
        if let Some(next_sync_at) = account.next_sync_at.as_deref() {
            if DateTime::parse_from_rfc3339(next_sync_at)
                .map(|value| value.with_timezone(&Utc) > Utc::now())
                .unwrap_or(false)
            {
                return;
            }
        }
        let Ok(_guard) = self.sync_lock.try_lock() else {
            return;
        };
        let _ = self.sync_locked().await;
    }

    async fn sync_locked(&self) -> Result<GoogleSyncSummary, GoogleError> {
        let account = self.account().await?;
        sqlx::query("UPDATE connector_accounts SET state = 'syncing', last_error_code = NULL, updated_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(&account.id)
            .execute(&self.pool)
            .await?;
        let result = self.sync_providers(&account).await;
        match result {
            Ok(()) => {
                let completed = Utc::now();
                let next = completed + Duration::minutes(FIVE_MINUTES);
                let updated = sqlx::query("UPDATE connector_accounts SET state = 'connected', last_sync_at = ?, next_sync_at = ?, retry_count = 0, last_error_code = NULL, updated_at = ? WHERE id = ? AND generation = ?")
                    .bind(completed.to_rfc3339())
                    .bind(next.to_rfc3339())
                    .bind(completed.to_rfc3339())
                    .bind(&account.id)
                    .bind(account.generation)
                    .execute(&self.pool)
                    .await?;
                if updated.rows_affected() == 0 {
                    return Err(GoogleError::NotConnected);
                }
                let (gmail_message_count, calendar_event_count) =
                    self.item_counts(&account.id).await?;
                Ok(GoogleSyncSummary {
                    gmail_message_count,
                    calendar_event_count,
                    completed_at: completed.to_rfc3339(),
                })
            }
            Err(error) => {
                self.set_account_error(&account.id, &error).await?;
                Err(error)
            }
        }
    }

    async fn sync_providers(&self, account: &AccountRow) -> Result<(), GoogleError> {
        let access_token = self.access_token(account).await?;
        let data_key = load_data_key(&account.id)?;
        self.sync_gmail(account, &access_token, &data_key).await?;
        self.sync_calendar(account, &access_token, &data_key)
            .await?;
        Ok(())
    }

    async fn sync_gmail(
        &self,
        account: &AccountRow,
        access_token: &str,
        data_key: &[u8; 32],
    ) -> Result<(), GoogleError> {
        if let Some(history_id) = account.gmail_history_id.as_deref() {
            if self
                .sync_gmail_incremental(&account.id, access_token, data_key, history_id)
                .await?
            {
                return Ok(());
            }
        }
        self.sync_gmail_full(&account.id, access_token, data_key)
            .await
    }

    async fn sync_gmail_full(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
    ) -> Result<(), GoogleError> {
        let mut ids = Vec::new();
        let mut page_token: Option<String> = None;
        while ids.len() < GMAIL_LIMIT {
            let mut request = self
                .client
                .get(format!("{}/users/me/messages", self.endpoints.gmail))
                .bearer_auth(access_token)
                .query(&[
                    ("q", "{in:inbox in:sent} newer_than:30d"),
                    ("maxResults", "100"),
                ]);
            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }
            let page: GmailListResponse =
                checked_response(request.send().await.map_err(|_| GoogleError::Network)?)?
                    .json()
                    .await
                    .map_err(|_| GoogleError::Provider)?;
            ids.extend(page.messages.into_iter().map(|message| message.id));
            ids.truncate(GMAIL_LIMIT);
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        let messages = self.fetch_gmail_messages(access_token, ids).await?;
        let profile: GmailProfile = self
            .authorized_get(
                &format!("{}/users/me/profile", self.endpoints.gmail),
                access_token,
            )
            .await?
            .json()
            .await
            .map_err(|_| GoogleError::Provider)?;
        let generation_id = Uuid::new_v4().to_string();
        let account_generation: i64 =
            sqlx::query_scalar("SELECT generation FROM connector_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await?;
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO ingest_generations (id, account_id, account_generation, source_kind, status, expected_items, created_at) VALUES (?, ?, ?, 'gmail_message', 'pending', ?, ?)")
            .bind(&generation_id)
            .bind(account_id)
            .bind(account_generation)
            .bind(messages.len() as i64)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        let present: HashSet<String> = messages.iter().map(|message| message.id.clone()).collect();
        for batch in messages.chunks(50) {
            let mut transaction = self.pool.begin().await?;
            for message in batch {
                persist_gmail_message(
                    &mut transaction,
                    account_id,
                    data_key,
                    message,
                    Some(&generation_id),
                )
                .await?;
            }
            sqlx::query("UPDATE ingest_generations SET committed_items = committed_items + ? WHERE id = ? AND status = 'pending'")
                .bind(batch.len() as i64)
                .bind(&generation_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
        }
        let existing: Vec<String> = sqlx::query_scalar("SELECT external_id FROM provider_items WHERE account_id = ? AND kind = 'gmail_message'")
            .bind(account_id)
            .fetch_all(&self.pool)
            .await?;
        let mut transaction = self.pool.begin().await?;
        for external_id in existing.into_iter().filter(|id| !present.contains(id)) {
            persist_tombstone_revision(
                &mut transaction,
                account_id,
                data_key,
                account_generation,
                &generation_id,
                "gmail_message",
                &external_id,
            )
            .await?;
            sqlx::query("DELETE FROM provider_items WHERE account_id = ? AND kind = 'gmail_message' AND external_id = ?")
                .bind(account_id)
                .bind(external_id)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(
            "UPDATE connector_accounts SET gmail_history_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(profile.history_id)
        .bind(Utc::now().to_rfc3339())
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE ingest_generations SET status = 'complete', completed_at = ? WHERE id = ? AND account_generation = (SELECT generation FROM connector_accounts WHERE id = ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(&generation_id)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn sync_gmail_incremental(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
        history_id: &str,
    ) -> Result<bool, GoogleError> {
        let mut added = Vec::new();
        let mut deleted = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest_history = history_id.to_owned();
        loop {
            let mut request = self
                .client
                .get(format!("{}/users/me/history", self.endpoints.gmail))
                .bearer_auth(access_token)
                .query(&[
                    ("startHistoryId", history_id),
                    ("historyTypes", "messageAdded"),
                    ("maxResults", "500"),
                ])
                .query(&[("historyTypes", "messageDeleted")]);
            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request.send().await.map_err(|_| GoogleError::Network)?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(false);
            }
            let page: GmailHistoryResponse = checked_response(response)?
                .json()
                .await
                .map_err(|_| GoogleError::Provider)?;
            if !page.history_id.is_empty() {
                latest_history = page.history_id;
            }
            for entry in page.history {
                added.extend(entry.messages_added.into_iter().map(|item| item.message.id));
                deleted.extend(
                    entry
                        .messages_deleted
                        .into_iter()
                        .map(|item| item.message.id),
                );
            }
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        added.sort();
        added.dedup();
        deleted.sort();
        deleted.dedup();
        let messages = self.fetch_gmail_messages(access_token, added).await?;
        let cutoff = (Utc::now() - Duration::days(30)).to_rfc3339();
        let aged_out: Vec<String> = sqlx::query_scalar("SELECT external_id FROM provider_items WHERE account_id = ? AND kind = 'gmail_message' AND occurred_at < ?")
            .bind(account_id)
            .bind(&cutoff)
            .fetch_all(&self.pool)
            .await?;
        deleted.extend(aged_out);
        deleted.sort();
        deleted.dedup();
        let generation_id = Uuid::new_v4().to_string();
        let account_generation: i64 =
            sqlx::query_scalar("SELECT generation FROM connector_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("INSERT INTO ingest_generations (id, account_id, account_generation, source_kind, status, expected_items, created_at) VALUES (?, ?, ?, 'gmail_message', 'pending', ?, ?)")
            .bind(&generation_id)
            .bind(account_id)
            .bind(account_generation)
            .bind((messages.len() + deleted.len()) as i64)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *transaction)
            .await?;
        for external_id in deleted {
            persist_tombstone_revision(
                &mut transaction,
                account_id,
                data_key,
                account_generation,
                &generation_id,
                "gmail_message",
                &external_id,
            )
            .await?;
            sqlx::query("DELETE FROM provider_items WHERE account_id = ? AND kind = 'gmail_message' AND external_id = ?")
                .bind(account_id)
                .bind(external_id)
                .execute(&mut *transaction)
                .await?;
        }
        for message in messages {
            persist_gmail_message(
                &mut transaction,
                account_id,
                data_key,
                &message,
                Some(&generation_id),
            )
            .await?;
        }
        sqlx::query(
            "UPDATE connector_accounts SET gmail_history_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(latest_history)
        .bind(Utc::now().to_rfc3339())
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE ingest_generations SET status = 'complete', committed_items = expected_items, completed_at = ? WHERE id = ? AND account_generation = (SELECT generation FROM connector_accounts WHERE id = ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(&generation_id)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn fetch_gmail_messages(
        &self,
        access_token: &str,
        ids: Vec<String>,
    ) -> Result<Vec<StoredGmailMessage>, GoogleError> {
        let messages: Vec<Option<StoredGmailMessage>> = stream::iter(ids)
            .map(|id| async move {
                let response = self
                    .client
                    .get(format!("{}/users/me/messages/{id}", self.endpoints.gmail))
                    .bearer_auth(access_token)
                    .query(&[("format", "full")])
                    .send()
                    .await
                    .map_err(|_| GoogleError::Network)?;
                if response.status() == StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                let message: GmailMessage = checked_response(response)?
                    .json()
                    .await
                    .map_err(|_| GoogleError::Provider)?;
                gmail_message_to_stored(message).map(Some)
            })
            .buffer_unordered(8)
            .try_collect()
            .await?;
        Ok(messages.into_iter().flatten().collect())
    }

    async fn sync_calendar(
        &self,
        account: &AccountRow,
        access_token: &str,
        data_key: &[u8; 32],
    ) -> Result<(), GoogleError> {
        let window_expired = account
            .calendar_window_anchor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc) < Utc::now() - Duration::days(1))
            .unwrap_or(true);
        if !window_expired {
            if let Some(sync_token) = account.calendar_sync_token.as_deref() {
                if self
                    .sync_calendar_incremental(&account.id, access_token, data_key, sync_token)
                    .await?
                {
                    return Ok(());
                }
            }
        }
        self.sync_calendar_full(&account.id, access_token, data_key)
            .await
    }

    async fn sync_calendar_full(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
    ) -> Result<(), GoogleError> {
        let anchor = Utc::now();
        let time_min = (anchor - Duration::days(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let time_max = (anchor + Duration::days(90)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let (events, sync_token) = self
            .fetch_calendar_pages(access_token, None, Some((&time_min, &time_max)))
            .await?
            .ok_or(GoogleError::Provider)?;
        let generation_id = Uuid::new_v4().to_string();
        let account_generation: i64 =
            sqlx::query_scalar("SELECT generation FROM connector_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await?;
        let present: HashSet<String> = events.iter().map(|event| event.id.clone()).collect();
        let existing: Vec<String> = sqlx::query_scalar("SELECT external_id FROM provider_items WHERE account_id = ? AND kind = 'calendar_event'")
            .bind(account_id)
            .fetch_all(&self.pool)
            .await?;
        let removed: Vec<String> = existing
            .into_iter()
            .filter(|external_id| !present.contains(external_id))
            .collect();
        let expected_items = events.len() + removed.len();
        sqlx::query("INSERT INTO ingest_generations (id, account_id, account_generation, source_kind, status, expected_items, created_at) VALUES (?, ?, ?, 'calendar_event', 'pending', ?, ?)")
            .bind(&generation_id)
            .bind(account_id)
            .bind(account_generation)
            .bind(expected_items as i64)
            .bind(anchor.to_rfc3339())
            .execute(&self.pool)
            .await?;

        for batch in events.chunks(100) {
            let mut transaction = self.pool.begin().await?;
            for event in batch {
                persist_calendar_event(
                    &mut transaction,
                    account_id,
                    data_key,
                    event,
                    Some(&generation_id),
                )
                .await?;
            }
            sqlx::query("UPDATE ingest_generations SET committed_items = committed_items + ? WHERE id = ? AND status = 'pending'")
                .bind(batch.len() as i64)
                .bind(&generation_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
        }
        let mut transaction = self.pool.begin().await?;
        for external_id in removed {
            persist_tombstone_revision(
                &mut transaction,
                account_id,
                data_key,
                account_generation,
                &generation_id,
                "calendar_event",
                &external_id,
            )
            .await?;
            sqlx::query("DELETE FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND external_id = ?")
                .bind(account_id)
                .bind(external_id)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("UPDATE connector_accounts SET calendar_sync_token = ?, calendar_window_anchor = ?, updated_at = ? WHERE id = ?")
            .bind(sync_token)
            .bind(anchor.to_rfc3339())
            .bind(anchor.to_rfc3339())
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE ingest_generations SET status = 'complete', committed_items = expected_items, completed_at = ? WHERE id = ? AND account_generation = (SELECT generation FROM connector_accounts WHERE id = ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(&generation_id)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn sync_calendar_incremental(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
        sync_token: &str,
    ) -> Result<bool, GoogleError> {
        let Some((events, next_sync_token)) = self
            .fetch_calendar_pages(access_token, Some(sync_token), None)
            .await?
        else {
            return Ok(false);
        };
        let generation_id = Uuid::new_v4().to_string();
        let account_generation: i64 =
            sqlx::query_scalar("SELECT generation FROM connector_accounts WHERE id = ?")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("INSERT INTO ingest_generations (id, account_id, account_generation, source_kind, status, expected_items, created_at) VALUES (?, ?, ?, 'calendar_event', 'pending', ?, ?)")
            .bind(&generation_id)
            .bind(account_id)
            .bind(account_generation)
            .bind(events.len() as i64)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *transaction)
            .await?;
        for event in events {
            persist_calendar_event(
                &mut transaction,
                account_id,
                data_key,
                &event,
                Some(&generation_id),
            )
            .await?;
        }
        sqlx::query(
            "UPDATE connector_accounts SET calendar_sync_token = ?, updated_at = ? WHERE id = ?",
        )
        .bind(next_sync_token)
        .bind(Utc::now().to_rfc3339())
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE ingest_generations SET status = 'complete', committed_items = expected_items, completed_at = ? WHERE id = ? AND account_generation = (SELECT generation FROM connector_accounts WHERE id = ?)")
            .bind(Utc::now().to_rfc3339())
            .bind(&generation_id)
            .bind(account_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn fetch_calendar_pages(
        &self,
        access_token: &str,
        sync_token: Option<&str>,
        window: Option<(&str, &str)>,
    ) -> Result<Option<(Vec<StoredCalendarEvent>, String)>, GoogleError> {
        let mut page_token: Option<String> = None;
        let mut events = Vec::new();
        let next_sync_token = loop {
            let mut request = self
                .client
                .get(format!(
                    "{}/calendars/primary/events",
                    self.endpoints.calendar
                ))
                .bearer_auth(access_token)
                .query(&[
                    ("singleEvents", "true"),
                    ("showDeleted", "true"),
                    ("maxResults", "2500"),
                ]);
            if let Some(token) = sync_token {
                request = request.query(&[("syncToken", token)]);
            }
            if let Some((time_min, time_max)) = window {
                request = request
                    .query(&[("timeMin", time_min), ("timeMax", time_max)])
                    .query(&[("orderBy", "startTime")]);
            }
            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request.send().await.map_err(|_| GoogleError::Network)?;
            if response.status() == StatusCode::GONE {
                return Ok(None);
            }
            let page: CalendarListResponse = checked_response(response)?
                .json()
                .await
                .map_err(|_| GoogleError::Provider)?;
            events.extend(page.items.into_iter().map(calendar_event_to_stored));
            page_token = page.next_page_token;
            if page_token.is_none() {
                break page.next_sync_token;
            }
        };
        Ok(Some((
            events,
            next_sync_token.ok_or(GoogleError::Provider)?,
        )))
    }

    pub async fn calendar_blocks(&self) -> Result<Vec<CalendarBlock>, GoogleError> {
        let account = match self.account().await {
            Ok(account) => account,
            Err(GoogleError::NotConnected) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let key = load_data_key(&account.id)?;
        let rows = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT nonce, ciphertext FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND status != 'cancelled' ORDER BY starts_at ASC",
        )
        .bind(&account.id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(nonce, ciphertext)| {
                let event: StoredCalendarEvent = decrypt_value(&key, &nonce, &ciphertext)?;
                Ok(stored_event_to_block(event))
            })
            .collect()
    }

    pub(crate) async fn ai_thread_source(
        &self,
        account_id: &str,
        thread_id: &str,
    ) -> Result<AiThreadSource, GoogleError> {
        let account = self.account().await?;
        if account.id != account_id {
            return Err(GoogleError::NotConnected);
        }
        let key = load_data_key(account_id)?;
        let account_email: String =
            decrypt_value(&key, &account.email_nonce, &account.email_ciphertext)?;
        let rows = sqlx::query_as::<_, (String, Vec<u8>, Vec<u8>)>(
            "SELECT latest_revision_id, nonce, ciphertext FROM provider_items WHERE account_id = ? AND kind = 'gmail_message' AND thread_id = ? AND latest_revision_id IS NOT NULL ORDER BY occurred_at ASC",
        )
        .bind(account_id)
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;
        let mut messages = Vec::with_capacity(rows.len());
        for (source_revision_id, nonce, ciphertext) in rows {
            let message: StoredGmailMessage = decrypt_value(&key, &nonce, &ciphertext)?;
            messages.push(AiGmailMessage {
                source_revision_id,
                from: message.from,
                to: message.to,
                cc: message.cc,
                body_text: message.body_text,
                occurred_at: message.occurred_at,
            });
        }
        Ok(AiThreadSource {
            account_email,
            messages,
        })
    }

    pub(crate) async fn ai_calendar_snapshot(
        &self,
        account_id: &str,
        event_id: &str,
    ) -> Result<AiCalendarSnapshot, GoogleError> {
        let account = self.account().await?;
        if account.id != account_id {
            return Err(GoogleError::NotConnected);
        }
        let key = load_data_key(account_id)?;
        let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT nonce, ciphertext FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND external_id = ?",
        )
        .bind(account_id)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(GoogleError::Validation(
            "That Calendar event is no longer available.".to_owned(),
        ))?;
        let event: StoredCalendarEvent = decrypt_value(&key, &row.0, &row.1)?;
        Ok(AiCalendarSnapshot {
            id: event.id,
            etag: event.etag,
            title: event.title,
            description: event.description,
            location: event.location,
            start_at: event.start_at,
            end_at: event.end_at,
            all_day: event.all_day,
            time_zone: event.time_zone,
            attendees: event.attendees,
            recurrence: event.recurrence,
        })
    }

    pub async fn mutate_calendar(
        &self,
        input: CalendarMutationInput,
    ) -> Result<CalendarMutationResult, GoogleError> {
        let account = self.account().await?;
        let access_token = self.access_token(&account).await?;
        let data_key = load_data_key(&account.id)?;
        let serialized = serde_json::to_vec(&input)
            .map_err(|_| GoogleError::Validation("Calendar mutation is not valid.".to_owned()))?;
        let payload_hash = format!("{:x}", Sha256::digest(&serialized));
        let (operation_id, action, target_external_id) = match &input {
            CalendarMutationInput::Create { operation_id, .. } => {
                (operation_id.clone(), "create", None)
            }
            CalendarMutationInput::Update {
                operation_id,
                event_id,
                ..
            } => (operation_id.clone(), "update", Some(event_id.clone())),
            CalendarMutationInput::Delete {
                operation_id,
                event_id,
                ..
            } => (operation_id.clone(), "delete", Some(event_id.clone())),
        };
        validate_operation_id(&operation_id)?;
        if let Some((existing_hash, existing_status, result_external_id)) = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT payload_hash, status, result_external_id FROM connector_mutations WHERE operation_id = ?",
        )
        .bind(&operation_id)
        .fetch_optional(&self.pool)
        .await?
        {
            if existing_hash != payload_hash {
                return Err(GoogleError::OperationMismatch);
            }
            if existing_status == "succeeded" {
                let event = match result_external_id {
                    Some(external_id) => self
                        .calendar_block_by_external(&account.id, &data_key, &external_id)
                        .await?,
                    None => None,
                };
                return Ok(CalendarMutationResult {
                    operation_id,
                    event,
                    deleted: action == "delete",
                });
            }
        } else {
            let now = Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO connector_mutations (operation_id, account_id, action, target_external_id, payload_hash, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'pending', ?, ?)")
                .bind(&operation_id)
                .bind(&account.id)
                .bind(action)
                .bind(&target_external_id)
                .bind(&payload_hash)
                .bind(&now)
                .bind(&now)
                .execute(&self.pool)
                .await?;
        }

        let result = match input {
            CalendarMutationInput::Create { event, .. } => {
                self.create_google_event(
                    &account.id,
                    &access_token,
                    &data_key,
                    &operation_id,
                    event,
                )
                .await
            }
            CalendarMutationInput::Update {
                event_id,
                expected_etag,
                patch,
                ..
            } => {
                self.update_google_event(
                    &account.id,
                    &access_token,
                    &data_key,
                    &operation_id,
                    &event_id,
                    &expected_etag,
                    patch,
                )
                .await
            }
            CalendarMutationInput::Delete {
                event_id,
                expected_etag,
                send_updates,
                ..
            } => {
                self.delete_google_event(
                    &account.id,
                    &access_token,
                    &data_key,
                    &operation_id,
                    &event_id,
                    &expected_etag,
                    &send_updates,
                )
                .await
            }
        };
        match &result {
            Err(GoogleError::Conflict) => {
                let _ = self.sync_now().await;
            }
            Err(GoogleError::Network | GoogleError::RateLimited) => {
                mark_mutation(
                    &self.pool,
                    &operation_id,
                    "pending",
                    None,
                    result.as_ref().err().map(GoogleError::code),
                )
                .await?;
            }
            Err(error) => {
                mark_mutation(
                    &self.pool,
                    &operation_id,
                    "failed",
                    None,
                    Some(error.code()),
                )
                .await?;
            }
            Ok(_) => {}
        }
        result
    }

    async fn create_google_event(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
        operation_id: &str,
        event: CalendarEventInput,
    ) -> Result<CalendarMutationResult, GoogleError> {
        validate_event_input(&event)?;
        let send_updates = validate_send_updates(&event.send_updates)?;
        let external_id = stable_google_event_id(operation_id);
        let payload = event_input_json(&event, Some(&external_id));
        let response = self
            .client
            .post(format!(
                "{}/calendars/primary/events",
                self.endpoints.calendar
            ))
            .bearer_auth(access_token)
            .query(&[("sendUpdates", send_updates)])
            .json(&payload)
            .send()
            .await;
        let remote = match response {
            Ok(response) if response.status() == StatusCode::CONFLICT => self
                .fetch_calendar_event(access_token, &external_id)
                .await?
                .ok_or(GoogleError::Provider)?,
            Ok(response) => checked_response(response)?
                .json::<GoogleCalendarEvent>()
                .await
                .map_err(|_| GoogleError::Provider)?,
            Err(_) => self
                .fetch_calendar_event(access_token, &external_id)
                .await?
                .ok_or(GoogleError::Network)?,
        };
        let stored = calendar_event_to_stored(remote);
        self.finish_calendar_mutation(account_id, data_key, operation_id, &stored)
            .await?;
        Ok(CalendarMutationResult {
            operation_id: operation_id.to_owned(),
            event: Some(stored_event_to_block(stored)),
            deleted: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_google_event(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
        operation_id: &str,
        event_id: &str,
        expected_etag: &str,
        patch: CalendarEventPatch,
    ) -> Result<CalendarMutationResult, GoogleError> {
        validate_expected_event_version(&self.pool, account_id, event_id, expected_etag).await?;
        validate_event_patch(&patch)?;
        let send_updates = validate_send_updates(&patch.send_updates)?;
        let payload = event_patch_json(&patch);
        let response = self
            .client
            .patch(calendar_event_url(&self.endpoints.calendar, event_id)?)
            .bearer_auth(access_token)
            .header("If-Match", expected_etag)
            .query(&[("sendUpdates", send_updates)])
            .json(&payload)
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        if response.status() == StatusCode::PRECONDITION_FAILED {
            mark_mutation(
                &self.pool,
                operation_id,
                "conflict",
                None,
                Some("calendar_conflict"),
            )
            .await?;
            return Err(GoogleError::Conflict);
        }
        let remote: GoogleCalendarEvent = checked_response(response)?
            .json()
            .await
            .map_err(|_| GoogleError::Provider)?;
        let stored = calendar_event_to_stored(remote);
        self.finish_calendar_mutation(account_id, data_key, operation_id, &stored)
            .await?;
        Ok(CalendarMutationResult {
            operation_id: operation_id.to_owned(),
            event: Some(stored_event_to_block(stored)),
            deleted: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn delete_google_event(
        &self,
        account_id: &str,
        access_token: &str,
        data_key: &[u8; 32],
        operation_id: &str,
        event_id: &str,
        expected_etag: &str,
        send_updates: &str,
    ) -> Result<CalendarMutationResult, GoogleError> {
        validate_expected_event_version(&self.pool, account_id, event_id, expected_etag).await?;
        let send_updates = validate_send_updates(send_updates)?;
        let response = self
            .client
            .delete(calendar_event_url(&self.endpoints.calendar, event_id)?)
            .bearer_auth(access_token)
            .header("If-Match", expected_etag)
            .query(&[("sendUpdates", send_updates)])
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        if response.status() == StatusCode::PRECONDITION_FAILED {
            mark_mutation(
                &self.pool,
                operation_id,
                "conflict",
                None,
                Some("calendar_conflict"),
            )
            .await?;
            return Err(GoogleError::Conflict);
        }
        if response.status() != StatusCode::NOT_FOUND {
            checked_response(response)?;
        }
        let mut transaction = self.pool.begin().await?;
        let tombstone = serde_json::to_vec(&json!({
            "deleted": true,
            "externalId": event_id,
            "providerVersion": expected_etag,
            "reason": "explicit_calendar_delete"
        }))
        .map_err(|_| GoogleError::Crypto)?;
        persist_source_revision(
            &mut transaction,
            account_id,
            data_key,
            None,
            "calendar_event",
            event_id,
            None,
            Some(expected_etag),
            None,
            true,
            &tombstone,
        )
        .await?;
        sqlx::query("DELETE FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND external_id = ?")
            .bind(account_id)
            .bind(event_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE connector_mutations SET status = 'succeeded', result_external_id = NULL, last_error_code = NULL, updated_at = ? WHERE operation_id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(operation_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(CalendarMutationResult {
            operation_id: operation_id.to_owned(),
            event: None,
            deleted: true,
        })
    }

    async fn finish_calendar_mutation(
        &self,
        account_id: &str,
        data_key: &[u8; 32],
        operation_id: &str,
        event: &StoredCalendarEvent,
    ) -> Result<(), GoogleError> {
        let mut transaction = self.pool.begin().await?;
        persist_calendar_event(&mut transaction, account_id, data_key, event, None).await?;
        sqlx::query("UPDATE connector_mutations SET status = 'succeeded', result_external_id = ?, last_error_code = NULL, updated_at = ? WHERE operation_id = ?")
            .bind(&event.id)
            .bind(Utc::now().to_rfc3339())
            .bind(operation_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn fetch_calendar_event(
        &self,
        access_token: &str,
        event_id: &str,
    ) -> Result<Option<GoogleCalendarEvent>, GoogleError> {
        let response = self
            .client
            .get(calendar_event_url(&self.endpoints.calendar, event_id)?)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| GoogleError::Network)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(
            checked_response(response)?
                .json()
                .await
                .map_err(|_| GoogleError::Provider)?,
        ))
    }

    async fn calendar_block_by_external(
        &self,
        account_id: &str,
        data_key: &[u8; 32],
        external_id: &str,
    ) -> Result<Option<CalendarBlock>, GoogleError> {
        let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
            "SELECT nonce, ciphertext FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND external_id = ?",
        )
        .bind(account_id)
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|(nonce, ciphertext)| {
            decrypt_value::<StoredCalendarEvent>(data_key, &nonce, &ciphertext)
                .map(stored_event_to_block)
        })
        .transpose()
    }
}

async fn persist_gmail_message(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &str,
    data_key: &[u8; 32],
    message: &StoredGmailMessage,
    ingest_generation_id: Option<&str>,
) -> Result<(), GoogleError> {
    let (nonce, ciphertext) = encrypt_value(data_key, message)?;
    let revision_payload = serde_json::to_vec(message).map_err(|_| GoogleError::Crypto)?;
    let revision_id = persist_source_revision(
        transaction,
        account_id,
        data_key,
        ingest_generation_id,
        "gmail_message",
        &message.id,
        Some(&message.thread_id),
        None,
        Some(&message.occurred_at),
        false,
        &revision_payload,
    )
    .await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO provider_items (id, account_id, kind, external_id, thread_id, occurred_at, status, nonce, ciphertext, latest_revision_id, ingest_generation_id, created_at, updated_at) VALUES (?, ?, 'gmail_message', ?, ?, ?, 'active', ?, ?, ?, ?, ?, ?) ON CONFLICT(account_id, kind, external_id) DO UPDATE SET thread_id = excluded.thread_id, occurred_at = excluded.occurred_at, nonce = excluded.nonce, ciphertext = excluded.ciphertext, latest_revision_id = excluded.latest_revision_id, ingest_generation_id = excluded.ingest_generation_id, updated_at = excluded.updated_at")
        .bind(format!("{account_id}:gmail:{}", message.id))
        .bind(account_id)
        .bind(&message.id)
        .bind(&message.thread_id)
        .bind(&message.occurred_at)
        .bind(nonce)
        .bind(ciphertext)
        .bind(revision_id)
        .bind(ingest_generation_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn persist_calendar_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &str,
    data_key: &[u8; 32],
    event: &StoredCalendarEvent,
    ingest_generation_id: Option<&str>,
) -> Result<(), GoogleError> {
    let (nonce, ciphertext) = encrypt_value(data_key, event)?;
    let revision_payload = serde_json::to_vec(event).map_err(|_| GoogleError::Crypto)?;
    let revision_id = persist_source_revision(
        transaction,
        account_id,
        data_key,
        ingest_generation_id,
        "calendar_event",
        &event.id,
        None,
        Some(&event.etag),
        Some(&event.start_at),
        event.status == "cancelled",
        &revision_payload,
    )
    .await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO provider_items (id, account_id, kind, external_id, etag, occurred_at, starts_at, ends_at, status, nonce, ciphertext, latest_revision_id, ingest_generation_id, created_at, updated_at) VALUES (?, ?, 'calendar_event', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(account_id, kind, external_id) DO UPDATE SET etag = excluded.etag, occurred_at = excluded.occurred_at, starts_at = excluded.starts_at, ends_at = excluded.ends_at, status = excluded.status, nonce = excluded.nonce, ciphertext = excluded.ciphertext, latest_revision_id = excluded.latest_revision_id, ingest_generation_id = excluded.ingest_generation_id, updated_at = excluded.updated_at")
        .bind(format!("{account_id}:calendar:{}", event.id))
        .bind(account_id)
        .bind(&event.id)
        .bind(&event.etag)
        .bind(&event.start_at)
        .bind(&event.start_at)
        .bind(&event.end_at)
        .bind(&event.status)
        .bind(nonce)
        .bind(ciphertext)
        .bind(revision_id)
        .bind(ingest_generation_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn persist_source_revision(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &str,
    data_key: &[u8; 32],
    ingest_generation_id: Option<&str>,
    kind: &str,
    external_id: &str,
    thread_id: Option<&str>,
    provider_version: Option<&str>,
    occurred_at: Option<&str>,
    tombstone: bool,
    plaintext: &[u8],
) -> Result<String, GoogleError> {
    let account_generation: i64 =
        sqlx::query_scalar("SELECT generation FROM connector_accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(&mut **transaction)
            .await?;
    let mut hash_input = plaintext.to_vec();
    if tombstone {
        hash_input.extend_from_slice(ingest_generation_id.unwrap_or_default().as_bytes());
    }
    let content_hash = format!("{:x}", Sha256::digest(&hash_input));
    let revision_id = format!("{account_id}:{kind}:{external_id}:{content_hash}");
    let (nonce, ciphertext) = encrypt_value(
        data_key,
        &serde_json::from_slice::<Value>(plaintext)
            .unwrap_or_else(|_| json!({"deleted": tombstone, "externalId": external_id})),
    )?;
    sqlx::query("INSERT OR IGNORE INTO provider_item_revisions (id, account_id, account_generation, ingest_generation_id, kind, external_id, thread_id, provider_version, content_hash, tombstone, occurred_at, nonce, ciphertext, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&revision_id)
        .bind(account_id)
        .bind(account_generation)
        .bind(ingest_generation_id)
        .bind(kind)
        .bind(external_id)
        .bind(thread_id)
        .bind(provider_version)
        .bind(&content_hash)
        .bind(tombstone)
        .bind(occurred_at)
        .bind(nonce)
        .bind(ciphertext)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut **transaction)
        .await?;

    if kind == "gmail_message" {
        if let Some(fingerprint) = sqlx::query_scalar::<_, String>("SELECT activation_fingerprint FROM ai_provider_configs WHERE selected = 1 AND state = 'ready' AND activation_fingerprint IS NOT NULL")
            .fetch_optional(&mut **transaction)
            .await?
        {
            let job_id = Uuid::new_v4().to_string();
            let idempotency_key = format!("extract:{revision_id}:{fingerprint}");
            let now = Utc::now().to_rfc3339();
            sqlx::query("INSERT OR IGNORE INTO ai_jobs (id, kind, account_id, account_generation, ingest_generation_id, source_revision_id, activation_fingerprint, idempotency_key, status, priority, not_before, created_at, updated_at) VALUES (?, 'extract_thread', ?, ?, ?, ?, ?, ?, 'queued', 70, ?, ?, ?)")
                .bind(job_id)
                .bind(account_id)
                .bind(account_generation)
                .bind(ingest_generation_id)
                .bind(&revision_id)
                .bind(&fingerprint)
                .bind(idempotency_key)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(&mut **transaction)
                .await?;
        }
    }
    Ok(revision_id)
}

#[allow(clippy::too_many_arguments)]
async fn persist_tombstone_revision(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &str,
    data_key: &[u8; 32],
    account_generation: i64,
    ingest_generation_id: &str,
    kind: &str,
    external_id: &str,
) -> Result<(), GoogleError> {
    let payload = serde_json::to_vec(&json!({
        "deleted": true,
        "externalId": external_id,
        "accountGeneration": account_generation,
    }))
    .map_err(|_| GoogleError::Crypto)?;
    persist_source_revision(
        transaction,
        account_id,
        data_key,
        Some(ingest_generation_id),
        kind,
        external_id,
        None,
        None,
        None,
        true,
        &payload,
    )
    .await?;
    Ok(())
}

fn gmail_message_to_stored(message: GmailMessage) -> Result<StoredGmailMessage, GoogleError> {
    let header = |name: &str| {
        message
            .payload
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
            .unwrap_or_default()
    };
    let mut plain = Vec::new();
    let mut html = Vec::new();
    collect_gmail_body(&message.payload, &mut plain, &mut html);
    let mut body_text = if !plain.is_empty() {
        plain.join("\n\n")
    } else if !html.is_empty() {
        html.into_iter()
            .filter_map(|value| html2text::from_read(value.as_bytes(), 100).ok())
            .map(|value| value.replace("**", "").replace("__", ""))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        message.snippet.clone()
    };
    body_text = body_text.trim().chars().take(250_000).collect();
    let occurred_at = message
        .internal_date
        .parse::<i64>()
        .ok()
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    Ok(StoredGmailMessage {
        id: message.id,
        thread_id: message.thread_id,
        subject: header("Subject"),
        from: header("From"),
        to: split_addresses(&header("To")),
        cc: split_addresses(&header("Cc")),
        body_text,
        snippet: message.snippet,
        occurred_at,
        labels: message.label_ids,
    })
}

fn collect_gmail_body(payload: &GmailPayload, plain: &mut Vec<String>, html: &mut Vec<String>) {
    if let Some(data) = payload.body.data.as_deref() {
        if let Ok(bytes) = URL_SAFE_NO_PAD
            .decode(data)
            .or_else(|_| URL_SAFE.decode(data))
        {
            let value = String::from_utf8_lossy(&bytes).into_owned();
            if payload.mime_type.eq_ignore_ascii_case("text/plain") {
                plain.push(value);
            } else if payload.mime_type.eq_ignore_ascii_case("text/html") {
                html.push(value);
            }
        }
    }
    for part in &payload.parts {
        collect_gmail_body(part, plain, html);
    }
}

fn split_addresses(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn calendar_event_to_stored(event: GoogleCalendarEvent) -> StoredCalendarEvent {
    let all_day = event.start.date.is_some();
    let start_at = event
        .start
        .date_time
        .clone()
        .or_else(|| event.start.date.as_deref().map(all_day_iso))
        .unwrap_or_default();
    let end_at = event
        .end
        .date_time
        .clone()
        .or_else(|| event.end.date.as_deref().map(all_day_iso))
        .unwrap_or_else(|| start_at.clone());
    StoredCalendarEvent {
        id: event.id,
        etag: event.etag,
        status: if event.status.is_empty() {
            "confirmed".to_owned()
        } else {
            event.status
        },
        title: if event.summary.is_empty() {
            "Untitled event".to_owned()
        } else {
            event.summary
        },
        description: event.description,
        location: event.location,
        start_at,
        end_at,
        all_day,
        time_zone: event.start.time_zone,
        attendees: event.attendees.into_iter().map(|item| item.email).collect(),
        recurrence: event.recurrence,
        recurring_event_id: event.recurring_event_id,
    }
}

fn all_day_iso(date: &str) -> String {
    format!("{date}T00:00:00Z")
}

fn stored_event_to_block(event: StoredCalendarEvent) -> CalendarBlock {
    let kind = if event.attendees.is_empty() {
        "execution"
    } else {
        "meeting"
    };
    CalendarBlock {
        id: format!("google:{}", event.id),
        title: event.title,
        start_at: event.start_at,
        end_at: event.end_at,
        kind: kind.to_owned(),
        color: if kind == "meeting" {
            "#b7b9b2".to_owned()
        } else {
            "#8ca481".to_owned()
        },
        origin: "google".to_owned(),
        external_id: Some(event.id),
        etag: Some(event.etag),
    }
}

fn event_input_json(event: &CalendarEventInput, id: Option<&str>) -> Value {
    let mut output = Map::new();
    if let Some(id) = id {
        output.insert("id".to_owned(), json!(id));
    }
    output.insert("summary".to_owned(), json!(event.title));
    if let Some(description) = &event.description {
        output.insert("description".to_owned(), json!(description));
    }
    if let Some(location) = &event.location {
        output.insert("location".to_owned(), json!(location));
    }
    let (start, end) = calendar_when_json(&event.when);
    output.insert("start".to_owned(), start);
    output.insert("end".to_owned(), end);
    if !event.attendees.is_empty() {
        output.insert(
            "attendees".to_owned(),
            Value::Array(
                event
                    .attendees
                    .iter()
                    .map(|email| json!({ "email": email }))
                    .collect(),
            ),
        );
    }
    if !event.recurrence.is_empty() {
        output.insert("recurrence".to_owned(), json!(event.recurrence));
    }
    Value::Object(output)
}

fn event_patch_json(patch: &CalendarEventPatch) -> Value {
    let mut output = Map::new();
    if let Some(title) = &patch.title {
        output.insert("summary".to_owned(), json!(title));
    }
    if let Some(description) = &patch.description {
        output.insert("description".to_owned(), json!(description));
    }
    if let Some(location) = &patch.location {
        output.insert("location".to_owned(), json!(location));
    }
    if let Some(when) = &patch.when {
        let (start, end) = calendar_when_json(when);
        output.insert("start".to_owned(), start);
        output.insert("end".to_owned(), end);
    }
    if let Some(attendees) = &patch.attendees {
        output.insert(
            "attendees".to_owned(),
            Value::Array(
                attendees
                    .iter()
                    .map(|email| json!({ "email": email }))
                    .collect(),
            ),
        );
    }
    if let Some(recurrence) = &patch.recurrence {
        output.insert("recurrence".to_owned(), json!(recurrence));
    }
    Value::Object(output)
}

fn calendar_when_json(when: &CalendarWhen) -> (Value, Value) {
    match when {
        CalendarWhen::Timed {
            start_at,
            end_at,
            time_zone,
        } => (
            json!({ "dateTime": start_at, "timeZone": time_zone }),
            json!({ "dateTime": end_at, "timeZone": time_zone }),
        ),
        CalendarWhen::AllDay {
            start_date,
            end_date,
        } => (json!({ "date": start_date }), json!({ "date": end_date })),
    }
}

fn validate_event_input(event: &CalendarEventInput) -> Result<(), GoogleError> {
    validate_title(&event.title)?;
    validate_calendar_when(&event.when)?;
    validate_attendees(&event.attendees)
}

fn validate_event_patch(patch: &CalendarEventPatch) -> Result<(), GoogleError> {
    if let Some(title) = &patch.title {
        validate_title(title)?;
    }
    if let Some(when) = &patch.when {
        validate_calendar_when(when)?;
    }
    if let Some(attendees) = &patch.attendees {
        validate_attendees(attendees)?;
    }
    if event_patch_json(patch)
        .as_object()
        .is_none_or(Map::is_empty)
    {
        return Err(GoogleError::Validation(
            "Calendar updates must change at least one field.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<(), GoogleError> {
    let length = title.trim().chars().count();
    if !(1..=240).contains(&length) {
        return Err(GoogleError::Validation(
            "Calendar events need a title between 1 and 240 characters.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_calendar_when(when: &CalendarWhen) -> Result<(), GoogleError> {
    match when {
        CalendarWhen::Timed {
            start_at, end_at, ..
        } => {
            let start = DateTime::parse_from_rfc3339(start_at).map_err(|_| {
                GoogleError::Validation("Calendar start time is not valid.".to_owned())
            })?;
            let end = DateTime::parse_from_rfc3339(end_at).map_err(|_| {
                GoogleError::Validation("Calendar end time is not valid.".to_owned())
            })?;
            if end <= start {
                return Err(GoogleError::Validation(
                    "Calendar events must end after they start.".to_owned(),
                ));
            }
        }
        CalendarWhen::AllDay {
            start_date,
            end_date,
        } => {
            let start =
                chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d").map_err(|_| {
                    GoogleError::Validation("All-day start date is not valid.".to_owned())
                })?;
            let end = chrono::NaiveDate::parse_from_str(end_date, "%Y-%m-%d").map_err(|_| {
                GoogleError::Validation("All-day end date is not valid.".to_owned())
            })?;
            if end <= start {
                return Err(GoogleError::Validation(
                    "All-day events must end after they start.".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_attendees(attendees: &[String]) -> Result<(), GoogleError> {
    if attendees.iter().any(|email| {
        let email = email.trim();
        email.is_empty() || !email.contains('@') || email.chars().any(char::is_whitespace)
    }) {
        return Err(GoogleError::Validation(
            "Every attendee must have a valid email address.".to_owned(),
        ));
    }
    Ok(())
}

fn validate_send_updates(value: &str) -> Result<&str, GoogleError> {
    match value {
        "all" | "externalOnly" | "none" => Ok(value),
        _ => Err(GoogleError::Validation(
            "sendUpdates must be all, externalOnly, or none.".to_owned(),
        )),
    }
}

fn validate_operation_id(value: &str) -> Result<(), GoogleError> {
    if value.trim().is_empty() || value.len() > 200 {
        return Err(GoogleError::Validation(
            "Calendar operation IDs must be between 1 and 200 characters.".to_owned(),
        ));
    }
    Ok(())
}

fn stable_google_event_id(operation_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(operation_id.as_bytes()));
    format!("a{}", &digest[..31])
}

fn calendar_event_url(base: &str, event_id: &str) -> Result<Url, GoogleError> {
    let mut url = Url::parse(&format!("{base}/calendars/primary/events"))
        .map_err(|_| GoogleError::Provider)?;
    url.path_segments_mut()
        .map_err(|_| GoogleError::Provider)?
        .push(event_id);
    Ok(url)
}

async fn validate_expected_event_version(
    pool: &SqlitePool,
    account_id: &str,
    event_id: &str,
    expected_etag: &str,
) -> Result<(), GoogleError> {
    let current: Option<String> = sqlx::query_scalar("SELECT etag FROM provider_items WHERE account_id = ? AND kind = 'calendar_event' AND external_id = ?")
        .bind(account_id)
        .bind(event_id)
        .fetch_optional(pool)
        .await?;
    if current.as_deref() != Some(expected_etag) {
        return Err(GoogleError::Conflict);
    }
    Ok(())
}

async fn mark_mutation(
    pool: &SqlitePool,
    operation_id: &str,
    status: &str,
    result_external_id: Option<&str>,
    error_code: Option<&str>,
) -> Result<(), GoogleError> {
    sqlx::query("UPDATE connector_mutations SET status = ?, result_external_id = ?, last_error_code = ?, updated_at = ? WHERE operation_id = ?")
        .bind(status)
        .bind(result_external_id)
        .bind(error_code)
        .bind(Utc::now().to_rfc3339())
        .bind(operation_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug)]
struct OAuthCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn validate_oauth_callback(
    callback: OAuthCallback,
    expected_state: &str,
) -> Result<String, GoogleError> {
    if callback.state.as_deref() != Some(expected_state) {
        return Err(GoogleError::InvalidOAuthState);
    }
    if callback.error.is_some() {
        return Err(GoogleError::AuthorizationDenied);
    }
    callback.code.ok_or(GoogleError::AuthorizationDenied)
}

async fn receive_oauth_callback(listener: TcpListener) -> Result<OAuthCallback, GoogleError> {
    let (mut stream, _) = listener.accept().await.map_err(|_| GoogleError::Network)?;
    let mut request = vec![0_u8; 8192];
    let size = stream
        .read(&mut request)
        .await
        .map_err(|_| GoogleError::Network)?;
    let request = String::from_utf8_lossy(&request[..size]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or(GoogleError::Provider)?;
    let url =
        Url::parse(&format!("http://localhost{target}")).map_err(|_| GoogleError::Provider)?;
    let parameters: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let success = parameters.contains_key("code") && !parameters.contains_key("error");
    let heading = if success {
        "Google connected"
    } else {
        "Connection cancelled"
    };
    let message = if success {
        "Return to Kyra. This window can be closed."
    } else {
        "Return to Kyra and try again when ready."
    };
    let body = format!("<!doctype html><meta charset=\"utf-8\"><title>{heading}</title><style>body{{font:16px system-ui;background:#1b1d1a;color:#eee;display:grid;place-content:center;height:100vh;margin:0}}main{{text-align:center}}p{{color:#aaa}}</style><main><h1>{heading}</h1><p>{message}</p></main>");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    Ok(OAuthCallback {
        code: parameters.get("code").cloned(),
        state: parameters.get("state").cloned(),
        error: parameters.get("error").cloned(),
    })
}

fn random_urlsafe(bytes: usize) -> String {
    let mut output = Vec::with_capacity(bytes);
    while output.len() < bytes {
        output.extend_from_slice(Uuid::new_v4().as_bytes());
    }
    output.truncate(bytes);
    URL_SAFE_NO_PAD.encode(output)
}

fn generate_data_key() -> [u8; 32] {
    let key = Key::<Aes256Gcm>::generate();
    let mut output = [0_u8; 32];
    output.copy_from_slice(key.as_slice());
    output
}

fn encrypt_value<T: Serialize>(
    key: &[u8; 32],
    value: &T,
) -> Result<(Vec<u8>, Vec<u8>), GoogleError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| GoogleError::Crypto)?;
    let nonce = Nonce::generate();
    let plaintext = serde_json::to_vec(value).map_err(|_| GoogleError::Crypto)?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|_| GoogleError::Crypto)?;
    Ok((nonce.as_slice().to_vec(), ciphertext))
}

fn decrypt_value<T: DeserializeOwned>(
    key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<T, GoogleError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| GoogleError::Crypto)?;
    let nonce_bytes: [u8; 12] = nonce.try_into().map_err(|_| GoogleError::Crypto)?;
    let plaintext = cipher
        .decrypt(&Nonce::from(nonce_bytes), ciphertext)
        .map_err(|_| GoogleError::Crypto)?;
    serde_json::from_slice(&plaintext).map_err(|_| GoogleError::Crypto)
}

fn keychain_entry(account_id: &str, kind: &str) -> Result<Entry, GoogleError> {
    Entry::new(GOOGLE_KEYCHAIN_SERVICE, &format!("{account_id}:{kind}"))
        .map_err(|_| GoogleError::Keychain)
}

fn store_refresh_token(account_id: &str, token: &str) -> Result<(), GoogleError> {
    keychain_entry(account_id, "refresh_token")?
        .set_password(token)
        .map_err(|_| GoogleError::Keychain)
}

fn load_refresh_token(account_id: &str) -> Result<String, GoogleError> {
    keychain_entry(account_id, "refresh_token")?
        .get_password()
        .map_err(|_| GoogleError::ReconnectRequired)
}

fn store_data_key(account_id: &str, key: &[u8; 32]) -> Result<(), GoogleError> {
    keychain_entry(account_id, "data_key")?
        .set_secret(key)
        .map_err(|_| GoogleError::Keychain)
}

fn load_data_key(account_id: &str) -> Result<[u8; 32], GoogleError> {
    let secret = keychain_entry(account_id, "data_key")?
        .get_secret()
        .map_err(|_| GoogleError::Keychain)?;
    secret.try_into().map_err(|_| GoogleError::Crypto)
}

fn delete_secret(account_id: &str, kind: &str) -> Result<(), GoogleError> {
    match keychain_entry(account_id, kind)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(_) => Err(GoogleError::Keychain),
    }
}

fn checked_response(response: Response) -> Result<Response, GoogleError> {
    match response.status() {
        status if status.is_success() => Ok(response),
        StatusCode::UNAUTHORIZED => Err(GoogleError::ReconnectRequired),
        StatusCode::TOO_MANY_REQUESTS => Err(GoogleError::RateLimited),
        StatusCode::PRECONDITION_FAILED => Err(GoogleError::Conflict),
        status if status.is_server_error() => Err(GoogleError::Network),
        _ => Err(GoogleError::Provider),
    }
}

fn public_error_for_code(code: String) -> String {
    match code.as_str() {
        "missing_client_id" => "Add KYRA_GOOGLE_CLIENT_ID to .env.local before connecting Google.",
        "authorization_denied" => "Google authorization was cancelled.",
        "invalid_oauth_state" => "The Google authorization response could not be trusted.",
        "oauth_timeout" => "Google authorization timed out. Try connecting again.",
        "reconnect_required" => "Google access expired or was revoked. Reconnect the account.",
        "rate_limited" => "Google is temporarily limiting requests. Kyra will retry automatically.",
        "network" => "Kyra could not reach Google. Cached data is still available.",
        "keychain" => "Kyra could not use macOS Keychain.",
        "crypto" => "Kyra could not decrypt connected data.",
        _ => "Google synchronization needs attention.",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path, query_param, query_param_is_missing},
        Mock, MockServer, ResponseTemplate,
    };

    fn gmail_message(payload: GmailPayload) -> GmailMessage {
        GmailMessage {
            id: "message-1".to_owned(),
            thread_id: "thread-1".to_owned(),
            label_ids: vec!["INBOX".to_owned()],
            snippet: "fallback".to_owned(),
            internal_date: "1723852800000".to_owned(),
            payload,
        }
    }

    async fn insert_test_account(pool: &SqlitePool, account_id: &str, key: &[u8; 32]) {
        let (nonce, ciphertext) = encrypt_value(key, &"test@example.com".to_owned()).unwrap();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO connector_accounts (id, provider, state, email_nonce, email_ciphertext, granted_scopes, next_sync_at, created_at, updated_at) VALUES (?, 'google', 'connected', ?, ?, ?, ?, ?, ?)")
            .bind(account_id)
            .bind(nonce)
            .bind(ciphertext)
            .bind(GOOGLE_SCOPES)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
    }

    #[test]
    fn encryption_round_trip_uses_unique_nonces_and_rejects_tampering() {
        let key = generate_data_key();
        let value = StoredGmailMessage {
            id: "m1".to_owned(),
            thread_id: "t1".to_owned(),
            subject: "Private".to_owned(),
            from: "a@example.com".to_owned(),
            to: vec!["b@example.com".to_owned()],
            cc: Vec::new(),
            body_text: "encrypted body".to_owned(),
            snippet: "encrypted".to_owned(),
            occurred_at: Utc::now().to_rfc3339(),
            labels: vec!["INBOX".to_owned()],
        };
        let (nonce_one, ciphertext_one) = encrypt_value(&key, &value).unwrap();
        let (nonce_two, _) = encrypt_value(&key, &value).unwrap();
        assert_ne!(nonce_one, nonce_two);
        let restored: StoredGmailMessage =
            decrypt_value(&key, &nonce_one, &ciphertext_one).unwrap();
        assert_eq!(restored.body_text, value.body_text);

        let mut tampered = ciphertext_one;
        tampered[0] ^= 0x80;
        assert!(matches!(
            decrypt_value::<StoredGmailMessage>(&key, &nonce_one, &tampered),
            Err(GoogleError::Crypto)
        ));
    }

    #[test]
    fn gmail_mime_prefers_plain_text_and_parses_headers() {
        let payload = GmailPayload {
            headers: vec![
                GmailHeader {
                    name: "Subject".to_owned(),
                    value: "Hello".to_owned(),
                },
                GmailHeader {
                    name: "From".to_owned(),
                    value: "Ada <ada@example.com>".to_owned(),
                },
                GmailHeader {
                    name: "To".to_owned(),
                    value: "one@example.com, two@example.com".to_owned(),
                },
            ],
            parts: vec![
                GmailPayload {
                    mime_type: "text/html".to_owned(),
                    body: GmailBody {
                        data: Some(URL_SAFE_NO_PAD.encode("<b>HTML</b>")),
                    },
                    ..Default::default()
                },
                GmailPayload {
                    mime_type: "text/plain".to_owned(),
                    body: GmailBody {
                        data: Some(URL_SAFE_NO_PAD.encode("Plain text")),
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let stored = gmail_message_to_stored(gmail_message(payload)).unwrap();
        assert_eq!(stored.subject, "Hello");
        assert_eq!(stored.body_text, "Plain text");
        assert_eq!(stored.to, vec!["one@example.com", "two@example.com"]);
    }

    #[test]
    fn gmail_mime_uses_html_then_snippet_fallback() {
        let html = GmailPayload {
            mime_type: "text/html".to_owned(),
            body: GmailBody {
                data: Some(URL_SAFE_NO_PAD.encode("<p>Only <strong>HTML</strong></p>")),
            },
            ..Default::default()
        };
        assert!(gmail_message_to_stored(gmail_message(html))
            .unwrap()
            .body_text
            .contains("Only HTML"));
        assert_eq!(
            gmail_message_to_stored(gmail_message(GmailPayload::default()))
                .unwrap()
                .body_text,
            "fallback"
        );
    }

    #[test]
    fn calendar_validation_covers_time_attendees_and_update_policy() {
        let valid = CalendarEventInput {
            title: "Review".to_owned(),
            description: None,
            location: None,
            when: CalendarWhen::Timed {
                start_at: "2026-08-17T10:00:00+05:30".to_owned(),
                end_at: "2026-08-17T11:00:00+05:30".to_owned(),
                time_zone: "Asia/Kolkata".to_owned(),
            },
            attendees: vec!["person@example.com".to_owned()],
            recurrence: Vec::new(),
            send_updates: "all".to_owned(),
        };
        assert!(validate_event_input(&valid).is_ok());
        assert!(validate_send_updates("externalOnly").is_ok());
        assert!(validate_send_updates("sometimes").is_err());

        let mut invalid = valid;
        invalid.attendees = vec!["not-an-email".to_owned()];
        assert!(matches!(
            validate_event_input(&invalid),
            Err(GoogleError::Validation(_))
        ));
    }

    #[test]
    fn stable_event_ids_are_deterministic_and_google_safe() {
        let first = stable_google_event_id("operation-123");
        assert_eq!(first, stable_google_event_id("operation-123"));
        assert_ne!(first, stable_google_event_id("operation-456"));
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn unavailable_key_is_reported_without_exposing_provider_content() {
        let pool = crate::db::memory_pool().await;
        let account_id = format!("missing-key-{}", Uuid::new_v4());
        insert_test_account(&pool, &account_id, &generate_data_key()).await;
        let connector = GoogleConnector::with_endpoints(pool, GoogleEndpoints::default());
        let status = connector.status().await.unwrap();
        assert_eq!(status.state, "error");
        assert_eq!(status.account_email, None);
        assert_eq!(
            status.last_error.as_deref(),
            Some("Kyra could not use macOS Keychain.")
        );
    }

    #[tokio::test]
    async fn disconnect_cascades_provider_items_and_mutation_records() {
        let pool = crate::db::memory_pool().await;
        let account_id = format!("disconnect-{}", Uuid::new_v4());
        let key = generate_data_key();
        insert_test_account(&pool, &account_id, &key).await;
        let message = StoredGmailMessage {
            id: "m1".to_owned(),
            thread_id: "t1".to_owned(),
            subject: "Secret subject".to_owned(),
            from: "one@example.com".to_owned(),
            to: Vec::new(),
            cc: Vec::new(),
            body_text: "Secret body".to_owned(),
            snippet: "Secret".to_owned(),
            occurred_at: Utc::now().to_rfc3339(),
            labels: vec!["INBOX".to_owned()],
        };
        let mut transaction = pool.begin().await.unwrap();
        persist_gmail_message(&mut transaction, &account_id, &key, &message, None)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let raw: Vec<u8> = sqlx::query_scalar(
            "SELECT ciphertext FROM provider_items WHERE account_id = ? LIMIT 1",
        )
        .bind(&account_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Secret body"));
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO connector_mutations (operation_id, account_id, action, payload_hash, status, created_at, updated_at) VALUES ('op-delete', ?, 'delete', 'hash', 'pending', ?, ?)")
            .bind(&account_id)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();

        let connector = GoogleConnector::with_endpoints(pool.clone(), GoogleEndpoints::default());
        assert_eq!(connector.disconnect().await.unwrap().state, "disconnected");
        let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connector_accounts")
            .fetch_one(&pool)
            .await
            .unwrap();
        let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mutations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connector_mutations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((accounts, items, mutations), (0, 0, 0));
    }

    #[tokio::test]
    async fn calendar_create_uses_stable_id_and_persists_confirmed_version() {
        let server = MockServer::start().await;
        let operation_id = "create-operation";
        let external_id = stable_google_event_id(operation_id);
        Mock::given(method("POST"))
            .and(path("/calendar/calendars/primary/events"))
            .and(query_param("sendUpdates", "all"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": external_id,
                "etag": "etag-v1",
                "status": "confirmed",
                "summary": "Native review",
                "start": {"dateTime": "2026-08-17T10:00:00+05:30", "timeZone": "Asia/Kolkata"},
                "end": {"dateTime": "2026-08-17T11:00:00+05:30", "timeZone": "Asia/Kolkata"},
                "attendees": [{"email": "person@example.com"}]
            })))
            .mount(&server)
            .await;
        let pool = crate::db::memory_pool().await;
        let account_id = format!("create-{}", Uuid::new_v4());
        let key = generate_data_key();
        insert_test_account(&pool, &account_id, &key).await;
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO connector_mutations (operation_id, account_id, action, payload_hash, status, created_at, updated_at) VALUES (?, ?, 'create', 'hash', 'pending', ?, ?)")
            .bind(operation_id)
            .bind(&account_id)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        let connector = GoogleConnector::with_endpoints(
            pool.clone(),
            GoogleEndpoints {
                calendar: format!("{}/calendar", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        let result = connector
            .create_google_event(
                &account_id,
                "token",
                &key,
                operation_id,
                CalendarEventInput {
                    title: "Native review".to_owned(),
                    description: None,
                    location: None,
                    when: CalendarWhen::Timed {
                        start_at: "2026-08-17T10:00:00+05:30".to_owned(),
                        end_at: "2026-08-17T11:00:00+05:30".to_owned(),
                        time_zone: "Asia/Kolkata".to_owned(),
                    },
                    attendees: vec!["person@example.com".to_owned()],
                    recurrence: Vec::new(),
                    send_updates: "all".to_owned(),
                },
            )
            .await
            .unwrap();
        let block = result.event.unwrap();
        assert_eq!(block.external_id.as_deref(), Some(external_id.as_str()));
        assert_eq!(block.etag.as_deref(), Some("etag-v1"));
        let mutation_status: String =
            sqlx::query_scalar("SELECT status FROM connector_mutations WHERE operation_id = ?")
                .bind(operation_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mutation_status, "succeeded");

        let requests = server.received_requests().await.unwrap();
        let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(payload["id"], external_id);
        assert_eq!(payload["attendees"][0]["email"], "person@example.com");
    }

    #[tokio::test]
    async fn calendar_update_rejects_remote_stale_version_and_marks_operation() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/calendar/calendars/primary/events/event-1"))
            .respond_with(ResponseTemplate::new(412))
            .mount(&server)
            .await;
        let pool = crate::db::memory_pool().await;
        let account_id = format!("update-{}", Uuid::new_v4());
        let key = generate_data_key();
        insert_test_account(&pool, &account_id, &key).await;
        let stored = StoredCalendarEvent {
            id: "event-1".to_owned(),
            etag: "etag-old".to_owned(),
            status: "confirmed".to_owned(),
            title: "Old title".to_owned(),
            description: None,
            location: None,
            start_at: "2026-08-17T10:00:00Z".to_owned(),
            end_at: "2026-08-17T11:00:00Z".to_owned(),
            all_day: false,
            time_zone: Some("UTC".to_owned()),
            attendees: Vec::new(),
            recurrence: Vec::new(),
            recurring_event_id: None,
        };
        let mut transaction = pool.begin().await.unwrap();
        persist_calendar_event(&mut transaction, &account_id, &key, &stored, None)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO connector_mutations (operation_id, account_id, action, target_external_id, payload_hash, status, created_at, updated_at) VALUES ('update-op', ?, 'update', 'event-1', 'hash', 'pending', ?, ?)")
            .bind(&account_id)
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        let connector = GoogleConnector::with_endpoints(
            pool.clone(),
            GoogleEndpoints {
                calendar: format!("{}/calendar", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        let result = connector
            .update_google_event(
                &account_id,
                "token",
                &key,
                "update-op",
                "event-1",
                "etag-old",
                CalendarEventPatch {
                    title: Some("New title".to_owned()),
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(result, Err(GoogleError::Conflict)));
        let operation: (String, Option<String>) = sqlx::query_as(
            "SELECT status, last_error_code FROM connector_mutations WHERE operation_id = 'update-op'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            operation,
            ("conflict".to_owned(), Some("calendar_conflict".to_owned()))
        );
    }

    #[test]
    fn calendar_event_url_escapes_external_ids() {
        let url = calendar_event_url("https://calendar.example/v3", "event/id").unwrap();
        assert_eq!(
            url.as_str(),
            "https://calendar.example/v3/calendars/primary/events/event%2Fid"
        );
    }

    #[tokio::test]
    async fn oauth_callback_parses_code_state_and_denial() {
        async fn callback_for(target: &str) -> OAuthCallback {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let target = target.to_owned();
            let client = tokio::spawn(async move {
                let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
                stream
                    .write_all(
                        format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes(),
                    )
                    .await
                    .unwrap();
            });
            let callback = receive_oauth_callback(listener).await.unwrap();
            client.await.unwrap();
            callback
        }

        let success = callback_for("/oauth/callback?code=abc&state=trusted").await;
        assert_eq!(success.code.as_deref(), Some("abc"));
        assert_eq!(success.state.as_deref(), Some("trusted"));
        assert_eq!(validate_oauth_callback(success, "trusted").unwrap(), "abc");
        let denied = callback_for("/oauth/callback?error=access_denied&state=trusted").await;
        assert_eq!(denied.error.as_deref(), Some("access_denied"));
        assert!(matches!(
            validate_oauth_callback(denied, "trusted"),
            Err(GoogleError::AuthorizationDenied)
        ));
        assert!(matches!(
            validate_oauth_callback(
                OAuthCallback {
                    code: Some("abc".to_owned()),
                    state: Some("attacker".to_owned()),
                    error: None,
                },
                "trusted",
            ),
            Err(GoogleError::InvalidOAuthState)
        ));
    }

    #[tokio::test]
    async fn gmail_full_message_fetch_normalizes_provider_payload() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/users/me/messages/m1"))
            .and(query_param("format", "full"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "m1",
                "threadId": "t1",
                "labelIds": ["INBOX"],
                "snippet": "hello",
                "internalDate": "1723852800000",
                "payload": {
                    "mimeType": "text/plain",
                    "headers": [{"name": "Subject", "value": "Fetched"}],
                    "body": {"data": URL_SAFE_NO_PAD.encode("complete body")}
                }
            })))
            .mount(&server)
            .await;
        let pool = crate::db::memory_pool().await;
        let connector = GoogleConnector::with_endpoints(
            pool,
            GoogleEndpoints {
                gmail: format!("{}/gmail", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        let messages = connector
            .fetch_gmail_messages("token", vec!["m1".to_owned()])
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Fetched");
        assert_eq!(messages[0].body_text, "complete body");
    }

    #[tokio::test]
    async fn malformed_gmail_payload_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/gmail/users/me/messages/bad"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        let connector = GoogleConnector::with_endpoints(
            crate::db::memory_pool().await,
            GoogleEndpoints {
                gmail: format!("{}/gmail", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        assert!(matches!(
            connector
                .fetch_gmail_messages("token", vec!["bad".to_owned()])
                .await,
            Err(GoogleError::Provider)
        ));
    }

    #[tokio::test]
    async fn calendar_sync_token_invalidation_requests_full_fallback() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/calendars/primary/events"))
            .and(query_param("syncToken", "expired"))
            .respond_with(ResponseTemplate::new(410))
            .mount(&server)
            .await;
        let connector = GoogleConnector::with_endpoints(
            crate::db::memory_pool().await,
            GoogleEndpoints {
                calendar: format!("{}/calendar", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        let result = connector
            .fetch_calendar_pages("token", Some("expired"), None)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn calendar_pagination_returns_all_events_and_final_sync_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendar/calendars/primary/events"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "event-1", "etag": "v1", "status": "confirmed", "summary": "First",
                    "start": {"dateTime": "2026-08-17T10:00:00Z"}, "end": {"dateTime": "2026-08-17T11:00:00Z"}
                }],
                "nextPageToken": "page-2"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/calendar/calendars/primary/events"))
            .and(query_param("pageToken", "page-2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "event-2", "etag": "v2", "status": "confirmed", "summary": "Second",
                    "start": {"date": "2026-08-18"}, "end": {"date": "2026-08-19"},
                    "recurrence": ["RRULE:FREQ=WEEKLY"]
                }],
                "nextSyncToken": "sync-final"
            })))
            .mount(&server)
            .await;
        let connector = GoogleConnector::with_endpoints(
            crate::db::memory_pool().await,
            GoogleEndpoints {
                calendar: format!("{}/calendar", server.uri()),
                ..GoogleEndpoints::default()
            },
        );
        let result = connector
            .fetch_calendar_pages(
                "token",
                None,
                Some(("2026-07-17T00:00:00Z", "2026-11-17T00:00:00Z")),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.0.len(), 2);
        assert_eq!(result.1, "sync-final");
        assert!(result.0[1].all_day);
        assert_eq!(result.0[1].recurrence, vec!["RRULE:FREQ=WEEKLY"]);
    }
}
