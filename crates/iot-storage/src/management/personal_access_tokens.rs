use std::{future::Future, pin::Pin};

use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Sqlite, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::{PlatformStore, PlatformStoreError};

/// A new personal access token whose plaintext was generated and hashed by the caller.
///
/// This storage boundary deliberately never accepts or returns a plaintext secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUserPersonalAccessToken {
    pub id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub token_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPersonalAccessTokenRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum UserPersonalAccessTokenRepositoryError {
    #[error("personal access token name is required")]
    EmptyName,
    #[error("personal access token prefix is required")]
    EmptyTokenPrefix,
    #[error("personal access token hash must be a SHA-256 hex digest")]
    InvalidTokenHash,
    #[error("user was not found in the active tenant")]
    UserNotFound,
    #[error("personal access token was not found")]
    TokenNotFound,
    #[error("personal access token prefix already exists")]
    TokenPrefixConflict,
    #[error("stored personal access token timestamp is invalid")]
    InvalidStoredTimestamp,
    #[error("personal access token storage operation failed")]
    Storage {
        #[source]
        source: PlatformStoreError,
    },
}

impl From<PlatformStoreError> for UserPersonalAccessTokenRepositoryError {
    fn from(source: PlatformStoreError) -> Self {
        Self::Storage { source }
    }
}

impl From<sqlx::Error> for UserPersonalAccessTokenRepositoryError {
    fn from(source: sqlx::Error) -> Self {
        Self::from(PlatformStoreError::from(source))
    }
}

pub trait UserPersonalAccessTokenRepository: Send + Sync {
    fn active_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<UserPersonalAccessTokenRecord>,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    >;

    fn rotate_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
        token: NewUserPersonalAccessToken,
        now: DateTime<Utc>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        UserPersonalAccessTokenRecord,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    >;

    fn revoke_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<(), UserPersonalAccessTokenRepositoryError>> + Send + 'a>>;

    fn resolve_user_personal_access_token<'a>(
        &'a self,
        token_hash: &'a str,
        now: DateTime<Utc>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<UserPersonalAccessTokenRecord>,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    >;
}

impl UserPersonalAccessTokenRepository for PlatformStore {
    fn active_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<UserPersonalAccessTokenRecord>,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { active_user_personal_access_token(self, tenant_id, user_id).await })
    }

    fn rotate_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
        token: NewUserPersonalAccessToken,
        now: DateTime<Utc>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        UserPersonalAccessTokenRecord,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            rotate_user_personal_access_token(self, tenant_id, user_id, token, now).await
        })
    }

    fn revoke_user_personal_access_token<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<(), UserPersonalAccessTokenRepositoryError>> + Send + 'a>>
    {
        Box::pin(
            async move { revoke_user_personal_access_token(self, tenant_id, user_id, now).await },
        )
    }

    fn resolve_user_personal_access_token<'a>(
        &'a self,
        token_hash: &'a str,
        now: DateTime<Utc>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<UserPersonalAccessTokenRecord>,
                        UserPersonalAccessTokenRepositoryError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { resolve_user_personal_access_token(self, token_hash, now).await })
    }
}

async fn active_user_personal_access_token(
    store: &PlatformStore,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<UserPersonalAccessTokenRecord>, UserPersonalAccessTokenRepositoryError> {
    match store {
        PlatformStore::Sqlite(store) => sqlx::query(
            "SELECT id, tenant_id, user_id, name, token_prefix, created_at, last_used_at, revoked_at
             FROM user_personal_access_tokens
             WHERE tenant_id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(tenant_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(store.pool())
        .await?
        .map(sqlite_record)
        .transpose(),
        PlatformStore::Timescale(pool) => sqlx::query(
            "SELECT id, tenant_id, user_id, name, token_prefix, created_at, last_used_at, revoked_at
             FROM user_personal_access_tokens
             WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(postgres_record)
        .transpose(),
    }
}

async fn rotate_user_personal_access_token(
    store: &PlatformStore,
    tenant_id: Uuid,
    user_id: Uuid,
    token: NewUserPersonalAccessToken,
    now: DateTime<Utc>,
) -> Result<UserPersonalAccessTokenRecord, UserPersonalAccessTokenRepositoryError> {
    validate_new_token(&token)?;
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            ensure_sqlite_account_active(&mut transaction, tenant_id, user_id).await?;
            sqlx::query(
                "UPDATE user_personal_access_tokens
                 SET revoked_at = ?
                 WHERE tenant_id = ? AND user_id = ? AND revoked_at IS NULL",
            )
            .bind(now.to_rfc3339())
            .bind(tenant_id.to_string())
            .bind(user_id.to_string())
            .execute(&mut *transaction)
            .await?;
            let record =
                insert_sqlite_token(&mut transaction, tenant_id, user_id, token, now).await?;
            transaction.commit().await?;
            Ok(record)
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            ensure_postgres_account_active(&mut transaction, tenant_id, user_id).await?;
            sqlx::query(
                "UPDATE user_personal_access_tokens
                 SET revoked_at = $1
                 WHERE tenant_id = $2 AND user_id = $3 AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(tenant_id)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
            let record =
                insert_postgres_token(&mut transaction, tenant_id, user_id, token, now).await?;
            transaction.commit().await?;
            Ok(record)
        }
    }
}

async fn revoke_user_personal_access_token(
    store: &PlatformStore,
    tenant_id: Uuid,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), UserPersonalAccessTokenRepositoryError> {
    let revoked = match store {
        PlatformStore::Sqlite(store) => sqlx::query(
            "UPDATE user_personal_access_tokens
             SET revoked_at = ?
             WHERE tenant_id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now.to_rfc3339())
        .bind(tenant_id.to_string())
        .bind(user_id.to_string())
        .execute(store.pool())
        .await?
        .rows_affected(),
        PlatformStore::Timescale(pool) => sqlx::query(
            "UPDATE user_personal_access_tokens
             SET revoked_at = $1
             WHERE tenant_id = $2 AND user_id = $3 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(tenant_id)
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected(),
    };
    if revoked == 0 {
        Err(UserPersonalAccessTokenRepositoryError::TokenNotFound)
    } else {
        Ok(())
    }
}

async fn resolve_user_personal_access_token(
    store: &PlatformStore,
    token_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<UserPersonalAccessTokenRecord>, UserPersonalAccessTokenRepositoryError> {
    if !is_sha256_hex(token_hash) {
        return Ok(None);
    }
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            let record = sqlx::query(
                "SELECT pat.id, pat.tenant_id, pat.user_id, pat.name, pat.token_prefix,
                        pat.created_at, pat.last_used_at, pat.revoked_at
                 FROM user_personal_access_tokens pat
                 INNER JOIN tenants t ON t.id = pat.tenant_id
                 INNER JOIN users user
                   ON user.id = pat.user_id AND user.tenant_id = pat.tenant_id
                 WHERE pat.token_hash = ? AND pat.revoked_at IS NULL
                   AND t.status = 'active'",
            )
            .bind(token_hash)
            .fetch_optional(&mut *transaction)
            .await?
            .map(sqlite_record)
            .transpose()?;
            let Some(mut record) = record else {
                transaction.commit().await?;
                return Ok(None);
            };
            let updated = sqlx::query(
                "UPDATE user_personal_access_tokens
                 SET last_used_at = ?
                 WHERE id = ? AND token_hash = ? AND revoked_at IS NULL
                   AND EXISTS (
                       SELECT 1 FROM tenants t
                       INNER JOIN users user
                         ON user.tenant_id = t.id
                       WHERE t.id = user_personal_access_tokens.tenant_id
                         AND user.id = user_personal_access_tokens.user_id
                         AND t.status = 'active'
                   )",
            )
            .bind(now.to_rfc3339())
            .bind(record.id.to_string())
            .bind(token_hash)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            transaction.commit().await?;
            if updated == 0 {
                Ok(None)
            } else {
                record.last_used_at = Some(now);
                Ok(Some(record))
            }
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            let record = sqlx::query(
                "SELECT pat.id, pat.tenant_id, pat.user_id, pat.name, pat.token_prefix,
                        pat.created_at, pat.last_used_at, pat.revoked_at
                 FROM user_personal_access_tokens pat
                 INNER JOIN tenants t ON t.id = pat.tenant_id
                 INNER JOIN users user
                   ON user.id = pat.user_id AND user.tenant_id = pat.tenant_id
                 WHERE pat.token_hash = $1 AND pat.revoked_at IS NULL
                   AND t.status = 'active'
                 FOR UPDATE OF pat",
            )
            .bind(token_hash)
            .fetch_optional(&mut *transaction)
            .await?
            .map(postgres_record)
            .transpose()?;
            let Some(mut record) = record else {
                transaction.commit().await?;
                return Ok(None);
            };
            let updated = sqlx::query(
                "UPDATE user_personal_access_tokens
                 SET last_used_at = $1
                 WHERE id = $2 AND token_hash = $3 AND revoked_at IS NULL
                   AND EXISTS (
                       SELECT 1 FROM tenants t
                       INNER JOIN users user
                         ON user.tenant_id = t.id
                       WHERE t.id = user_personal_access_tokens.tenant_id
                         AND user.id = user_personal_access_tokens.user_id
                         AND t.status = 'active'
                   )",
            )
            .bind(now)
            .bind(record.id)
            .bind(token_hash)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            transaction.commit().await?;
            if updated == 0 {
                Ok(None)
            } else {
                record.last_used_at = Some(now);
                Ok(Some(record))
            }
        }
    }
}

async fn ensure_sqlite_account_active(
    transaction: &mut Transaction<'_, Sqlite>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), UserPersonalAccessTokenRepositoryError> {
    let found = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM users user
         INNER JOIN tenants t ON t.id = user.tenant_id
         WHERE user.id = ? AND user.tenant_id = ?
           AND t.status = 'active'",
    )
    .bind(user_id.to_string())
    .bind(tenant_id.to_string())
    .fetch_optional(&mut **transaction)
    .await?;
    found
        .ok_or(UserPersonalAccessTokenRepositoryError::UserNotFound)
        .map(|_| ())
}

async fn ensure_postgres_account_active(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), UserPersonalAccessTokenRepositoryError> {
    let found = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM users user
         INNER JOIN tenants t ON t.id = user.tenant_id
         WHERE user.id = $1 AND user.tenant_id = $2
           AND t.status = 'active'
         FOR UPDATE OF user",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(&mut **transaction)
    .await?;
    found
        .ok_or(UserPersonalAccessTokenRepositoryError::UserNotFound)
        .map(|_| ())
}

async fn insert_sqlite_token(
    transaction: &mut Transaction<'_, Sqlite>,
    tenant_id: Uuid,
    user_id: Uuid,
    token: NewUserPersonalAccessToken,
    now: DateTime<Utc>,
) -> Result<UserPersonalAccessTokenRecord, UserPersonalAccessTokenRepositoryError> {
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO user_personal_access_tokens (
             id, tenant_id, user_id, name, token_prefix, token_hash, created_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(token.id.to_string())
    .bind(tenant_id.to_string())
    .bind(user_id.to_string())
    .bind(&token.name)
    .bind(&token.token_prefix)
    .bind(&token.token_hash)
    .bind(now.to_rfc3339())
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Err(UserPersonalAccessTokenRepositoryError::TokenPrefixConflict);
    }
    Ok(record_from_new(token, tenant_id, user_id, now))
}

async fn insert_postgres_token(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
    token: NewUserPersonalAccessToken,
    now: DateTime<Utc>,
) -> Result<UserPersonalAccessTokenRecord, UserPersonalAccessTokenRepositoryError> {
    let inserted = sqlx::query(
        "INSERT INTO user_personal_access_tokens (
             id, tenant_id, user_id, name, token_prefix, token_hash, created_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (token_prefix) DO NOTHING
         RETURNING id",
    )
    .bind(token.id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(&token.name)
    .bind(&token.token_prefix)
    .bind(&token.token_hash)
    .bind(now)
    .fetch_optional(&mut **transaction)
    .await?;
    if inserted.is_none() {
        return Err(UserPersonalAccessTokenRepositoryError::TokenPrefixConflict);
    }
    Ok(record_from_new(token, tenant_id, user_id, now))
}

fn record_from_new(
    token: NewUserPersonalAccessToken,
    tenant_id: Uuid,
    user_id: Uuid,
    created_at: DateTime<Utc>,
) -> UserPersonalAccessTokenRecord {
    UserPersonalAccessTokenRecord {
        id: token.id,
        tenant_id,
        user_id,
        name: token.name,
        token_prefix: token.token_prefix,
        created_at,
        last_used_at: None,
        revoked_at: None,
    }
}

fn validate_new_token(
    token: &NewUserPersonalAccessToken,
) -> Result<(), UserPersonalAccessTokenRepositoryError> {
    if token.name.trim().is_empty() {
        return Err(UserPersonalAccessTokenRepositoryError::EmptyName);
    }
    if token.token_prefix.is_empty() {
        return Err(UserPersonalAccessTokenRepositoryError::EmptyTokenPrefix);
    }
    if !is_sha256_hex(&token.token_hash) {
        return Err(UserPersonalAccessTokenRepositoryError::InvalidTokenHash);
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sqlite_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<UserPersonalAccessTokenRecord, UserPersonalAccessTokenRepositoryError> {
    Ok(UserPersonalAccessTokenRecord {
        id: parse_uuid(row.try_get("id")?)?,
        tenant_id: parse_uuid(row.try_get("tenant_id")?)?,
        user_id: parse_uuid(row.try_get("user_id")?)?,
        name: row.try_get("name")?,
        token_prefix: row.try_get("token_prefix")?,
        created_at: sqlite_timestamp(row.try_get("created_at")?)?,
        last_used_at: row
            .try_get::<Option<String>, _>("last_used_at")?
            .map(sqlite_timestamp)
            .transpose()?,
        revoked_at: row
            .try_get::<Option<String>, _>("revoked_at")?
            .map(sqlite_timestamp)
            .transpose()?,
    })
}

fn postgres_record(
    row: sqlx::postgres::PgRow,
) -> Result<UserPersonalAccessTokenRecord, UserPersonalAccessTokenRepositoryError> {
    Ok(UserPersonalAccessTokenRecord {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        user_id: row.try_get("user_id")?,
        name: row.try_get("name")?,
        token_prefix: row.try_get("token_prefix")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

fn parse_uuid(value: String) -> Result<Uuid, UserPersonalAccessTokenRepositoryError> {
    value
        .parse()
        .map_err(|_| UserPersonalAccessTokenRepositoryError::TokenNotFound)
}

fn sqlite_timestamp(
    value: String,
) -> Result<DateTime<Utc>, UserPersonalAccessTokenRepositoryError> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S")
                .map(|value| value.and_utc())
        })
        .map_err(|_| UserPersonalAccessTokenRepositoryError::InvalidStoredTimestamp)
}
