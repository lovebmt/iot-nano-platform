use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{
    AssertSqlSafe, Postgres, Row, Sqlite, Transaction, postgres::PgRow, sqlite::SqliteRow,
    types::Json,
};
use uuid::Uuid;

use crate::{
    BusinessError, BusinessFieldDefinition, BusinessFieldType, BusinessIndexedQuery,
    BusinessRecord, BusinessRecordPage, BusinessRecordQuery, BusinessRepository,
    BusinessTypeDefinition, BusinessUserPermissions, NewBusinessRecord, PlatformStore,
    PlatformStoreError, TenantActor, UpdateBusinessRecord,
};

impl From<sqlx::Error> for BusinessError {
    fn from(source: sqlx::Error) -> Self {
        Self::Storage {
            source: PlatformStoreError::from(source),
        }
    }
}

impl BusinessRepository for PlatformStore {
    fn upsert_business_type<'a>(
        &'a self,
        definition: BusinessTypeDefinition,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BusinessTypeDefinition, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { upsert_business_type(self, definition).await })
    }

    fn find_business_type<'a>(
        &'a self,
        tenant_id: Uuid,
        business_type_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<BusinessTypeDefinition>, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { find_business_type(self, tenant_id, business_type_id).await })
    }

    fn list_business_types<'a>(
        &'a self,
        tenant_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<BusinessTypeDefinition>, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { list_business_types(self, tenant_id).await })
    }

    fn list_business_types_for_asset<'a>(
        &'a self,
        tenant_id: Uuid,
        asset_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<BusinessTypeDefinition>, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { list_business_types_for_asset(self, tenant_id, asset_id).await })
    }

    fn delete_business_type<'a>(
        &'a self,
        tenant_id: Uuid,
        business_type_id: Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BusinessError>> + Send + 'a>>
    {
        Box::pin(async move { delete_business_type(self, tenant_id, business_type_id).await })
    }

    fn business_user_permissions<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<BusinessUserPermissions>, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { business_user_permissions(self, tenant_id, user_id).await })
    }

    fn set_business_user_permissions<'a>(
        &'a self,
        permissions: BusinessUserPermissions,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BusinessUserPermissions, BusinessError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { set_business_user_permissions(self, permissions).await })
    }

    fn create_business_record<'a>(
        &'a self,
        record: NewBusinessRecord,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BusinessRecord, BusinessError>> + Send + 'a>,
    > {
        Box::pin(async move { create_business_record(self, record).await })
    }

    fn update_business_record<'a>(
        &'a self,
        record: UpdateBusinessRecord,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BusinessRecord, BusinessError>> + Send + 'a>,
    > {
        Box::pin(async move { update_business_record(self, record).await })
    }

    fn delete_business_record<'a>(
        &'a self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BusinessError>> + Send + 'a>>
    {
        Box::pin(async move { delete_business_record(self, tenant_id, record_id).await })
    }

    fn query_business_records<'a>(
        &'a self,
        query: BusinessRecordQuery,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<BusinessRecordPage, BusinessError>> + Send + 'a,
        >,
    > {
        Box::pin(async move { query_business_records(self, query).await })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredBusinessField {
    key: String,
    label: String,
    field_type: String,
    #[serde(default)]
    options: Vec<String>,
    required: bool,
}

impl From<&BusinessFieldDefinition> for StoredBusinessField {
    fn from(field: &BusinessFieldDefinition) -> Self {
        let (field_type, options) = match &field.field_type {
            BusinessFieldType::String => ("string", Vec::new()),
            BusinessFieldType::Integer => ("integer", Vec::new()),
            BusinessFieldType::Decimal => ("decimal", Vec::new()),
            BusinessFieldType::Boolean => ("boolean", Vec::new()),
            BusinessFieldType::Date => ("date", Vec::new()),
            BusinessFieldType::DateTime => ("datetime", Vec::new()),
            BusinessFieldType::Select { options } => ("select", options.clone()),
        };
        Self {
            key: field.key.clone(),
            label: field.label.clone(),
            field_type: field_type.to_owned(),
            options,
            required: field.required,
        }
    }
}

impl TryFrom<StoredBusinessField> for BusinessFieldDefinition {
    type Error = BusinessError;

    fn try_from(field: StoredBusinessField) -> Result<Self, Self::Error> {
        let field_type = match field.field_type.as_str() {
            "string" => BusinessFieldType::String,
            "integer" => BusinessFieldType::Integer,
            "decimal" => BusinessFieldType::Decimal,
            "boolean" => BusinessFieldType::Boolean,
            "date" => BusinessFieldType::Date,
            "datetime" => BusinessFieldType::DateTime,
            "select" => BusinessFieldType::Select {
                options: field.options,
            },
            _ => return Err(BusinessError::InvalidFieldSchema),
        };
        Ok(Self {
            key: field.key,
            label: field.label,
            field_type,
            required: field.required,
        })
    }
}

#[derive(Debug, Clone)]
enum IndexValue {
    Integer(i64),
    Decimal(f64),
    Boolean(bool),
    Text(String),
}

async fn upsert_business_type(
    store: &PlatformStore,
    mut definition: BusinessTypeDefinition,
) -> Result<BusinessTypeDefinition, BusinessError> {
    definition.validate()?;
    if definition.id.is_nil() {
        definition.id = Uuid::now_v7();
        definition.version = 1;
    }

    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            let existing =
                sqlite_business_type(&mut *transaction, definition.tenant_id, definition.id)
                    .await?;
            if existing.is_some() {
                sqlite_assert_historical_field_types(&mut transaction, &definition).await?;
            }
            let saved = sqlite_upsert_business_type(&mut transaction, definition, existing).await?;
            transaction.commit().await?;
            Ok(saved)
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            let existing =
                postgres_business_type(&mut *transaction, definition.tenant_id, definition.id)
                    .await?;
            if existing.is_some() {
                postgres_assert_historical_field_types(&mut transaction, &definition).await?;
            }
            let saved =
                postgres_upsert_business_type(&mut transaction, definition, existing).await?;
            transaction.commit().await?;
            Ok(saved)
        }
    }
}

async fn sqlite_upsert_business_type(
    transaction: &mut Transaction<'_, Sqlite>,
    mut definition: BusinessTypeDefinition,
    existing: Option<BusinessTypeDefinition>,
) -> Result<BusinessTypeDefinition, BusinessError> {
    let (version, removed_fields, removed_profiles, index_changed) =
        prepare_type_update(&definition, existing.as_ref())?;
    definition.version = version;
    if let Some(previous) = &existing
        && index_changed
        && let Some(key) = &previous.indexed_field_key
    {
        drop_sqlite_business_expression_index(transaction, previous.tenant_id, previous.id, key)
            .await?;
    }
    let fields_json = serialize_fields(&definition.fields)?;
    let profiles_json = serialize_profile_ids(&definition.asset_profile_ids)?;
    let indexed_field_type = indexed_field_type(&definition);
    if existing.is_some() {
        sqlx::query(
            "UPDATE business_types
             SET name = ?, current_version = ?, indexed_field_key = ?, indexed_field_type = ?, updated_at = ?
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(&definition.name)
        .bind(definition.version)
        .bind(&definition.indexed_field_key)
        .bind(indexed_field_type)
        .bind(Utc::now().to_rfc3339())
        .bind(definition.id.to_string())
        .bind(definition.tenant_id.to_string())
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO business_types (
                id, tenant_id, name, current_version, indexed_field_key, indexed_field_type, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(definition.id.to_string())
        .bind(definition.tenant_id.to_string())
        .bind(&definition.name)
        .bind(definition.version)
        .bind(&definition.indexed_field_key)
        .bind(indexed_field_type)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&mut **transaction)
        .await?;
    }
    sqlx::query(
        "INSERT INTO business_type_versions (
            id, business_type_id, version, fields_json, asset_profile_ids_json, created_at
         ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(definition.id.to_string())
    .bind(definition.version)
    .bind(fields_json)
    .bind(profiles_json)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut **transaction)
    .await?;
    for field_key in removed_fields {
        remove_sqlite_business_field(transaction, definition.id, definition.version, &field_key)
            .await?;
    }
    for profile_id in removed_profiles {
        delete_sqlite_profile_business_records(
            transaction,
            definition.tenant_id,
            definition.id,
            profile_id,
        )
        .await?;
    }
    if (existing.is_none() || index_changed) && definition.indexed_field_key.is_some() {
        create_sqlite_business_expression_index(transaction, &definition).await?;
    }
    Ok(definition)
}

async fn postgres_upsert_business_type(
    transaction: &mut Transaction<'_, Postgres>,
    mut definition: BusinessTypeDefinition,
    existing: Option<BusinessTypeDefinition>,
) -> Result<BusinessTypeDefinition, BusinessError> {
    let (version, removed_fields, removed_profiles, index_changed) =
        prepare_type_update(&definition, existing.as_ref())?;
    definition.version = version;
    if let Some(previous) = &existing
        && index_changed
        && let Some(key) = &previous.indexed_field_key
    {
        drop_postgres_business_expression_index(transaction, previous.tenant_id, previous.id, key)
            .await?;
    }
    let fields_json = json_fields(&definition.fields)?;
    let profiles_json = json_profile_ids(&definition.asset_profile_ids)?;
    let indexed_field_type = indexed_field_type(&definition);
    if existing.is_some() {
        sqlx::query(
            "UPDATE business_types
             SET name = $1, current_version = $2, indexed_field_key = $3, indexed_field_type = $4,
                 updated_at = now()
             WHERE id = $5 AND tenant_id = $6",
        )
        .bind(&definition.name)
        .bind(definition.version)
        .bind(&definition.indexed_field_key)
        .bind(indexed_field_type)
        .bind(definition.id)
        .bind(definition.tenant_id)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO business_types (
                id, tenant_id, name, current_version, indexed_field_key, indexed_field_type
             ) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(definition.id)
        .bind(definition.tenant_id)
        .bind(&definition.name)
        .bind(definition.version)
        .bind(&definition.indexed_field_key)
        .bind(indexed_field_type)
        .execute(&mut **transaction)
        .await?;
    }
    sqlx::query(
        "INSERT INTO business_type_versions (
            id, business_type_id, version, fields_json, asset_profile_ids_json
         ) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(definition.id)
    .bind(definition.version)
    .bind(Json(fields_json))
    .bind(Json(profiles_json))
    .execute(&mut **transaction)
    .await?;
    for field_key in removed_fields {
        remove_postgres_business_field(transaction, definition.id, definition.version, &field_key)
            .await?;
    }
    for profile_id in removed_profiles {
        delete_postgres_profile_business_records(
            transaction,
            definition.tenant_id,
            definition.id,
            profile_id,
        )
        .await?;
    }
    if (existing.is_none() || index_changed) && definition.indexed_field_key.is_some() {
        create_postgres_business_expression_index(transaction, &definition).await?;
    }
    Ok(definition)
}

fn prepare_type_update(
    next: &BusinessTypeDefinition,
    existing: Option<&BusinessTypeDefinition>,
) -> Result<(i64, Vec<String>, Vec<Uuid>, bool), BusinessError> {
    let Some(previous) = existing else {
        return Ok((1, Vec::new(), Vec::new(), next.indexed_field_key.is_some()));
    };
    for old_field in &previous.fields {
        if let Some(new_field) = next.fields.iter().find(|field| field.key == old_field.key)
            && new_field.field_type != old_field.field_type
        {
            return Err(BusinessError::InvalidFieldSchema);
        }
    }
    let current_keys = next
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<HashSet<_>>();
    let removed_fields = previous
        .fields
        .iter()
        .filter(|field| !current_keys.contains(field.key.as_str()))
        .map(|field| field.key.clone())
        .collect();
    let current_profiles = next
        .asset_profile_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let removed_profiles = previous
        .asset_profile_ids
        .iter()
        .copied()
        .filter(|profile| !current_profiles.contains(profile))
        .collect();
    let index_changed = previous.indexed_field_key != next.indexed_field_key;
    Ok((
        previous.version + 1,
        removed_fields,
        removed_profiles,
        index_changed,
    ))
}

async fn sqlite_assert_historical_field_types(
    transaction: &mut Transaction<'_, Sqlite>,
    next: &BusinessTypeDefinition,
) -> Result<(), BusinessError> {
    let versions = sqlx::query_scalar::<_, String>(
        "SELECT fields_json FROM business_type_versions WHERE business_type_id = ?",
    )
    .bind(next.id.to_string())
    .fetch_all(&mut **transaction)
    .await?;
    for fields_json in versions {
        assert_field_types_are_immutable(next, &deserialize_fields(&fields_json)?)?;
    }
    Ok(())
}

async fn postgres_assert_historical_field_types(
    transaction: &mut Transaction<'_, Postgres>,
    next: &BusinessTypeDefinition,
) -> Result<(), BusinessError> {
    let versions = sqlx::query_scalar::<_, Json<Value>>(
        "SELECT fields_json FROM business_type_versions WHERE business_type_id = $1",
    )
    .bind(next.id)
    .fetch_all(&mut **transaction)
    .await?;
    for fields_json in versions {
        assert_field_types_are_immutable(next, &deserialize_json_fields(fields_json.0)?)?;
    }
    Ok(())
}

fn assert_field_types_are_immutable(
    next: &BusinessTypeDefinition,
    previous_fields: &[BusinessFieldDefinition],
) -> Result<(), BusinessError> {
    for previous in previous_fields {
        if let Some(next_field) = next.fields.iter().find(|field| field.key == previous.key)
            && next_field.field_type != previous.field_type
        {
            return Err(BusinessError::InvalidFieldSchema);
        }
    }
    Ok(())
}

async fn find_business_type(
    store: &PlatformStore,
    tenant_id: Uuid,
    business_type_id: Uuid,
) -> Result<Option<BusinessTypeDefinition>, BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            sqlite_business_type(store.pool(), tenant_id, business_type_id).await
        }
        PlatformStore::Timescale(pool) => {
            postgres_business_type(pool, tenant_id, business_type_id).await
        }
    }
}

async fn sqlite_business_type<'e, E>(
    executor: E,
    tenant_id: Uuid,
    business_type_id: Uuid,
) -> Result<Option<BusinessTypeDefinition>, BusinessError>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(
        "SELECT types.id, types.tenant_id, types.name, types.current_version,
                types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
         FROM business_types AS types
         JOIN business_type_versions AS versions
           ON versions.business_type_id = types.id AND versions.version = types.current_version
         WHERE types.id = ? AND types.tenant_id = ?",
    )
    .bind(business_type_id.to_string())
    .bind(tenant_id.to_string())
    .fetch_optional(executor)
    .await?;
    row.map(sqlite_business_type_from_row).transpose()
}

async fn postgres_business_type<'e, E>(
    executor: E,
    tenant_id: Uuid,
    business_type_id: Uuid,
) -> Result<Option<BusinessTypeDefinition>, BusinessError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        "SELECT types.id, types.tenant_id, types.name, types.current_version,
                types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
         FROM business_types AS types
         JOIN business_type_versions AS versions
           ON versions.business_type_id = types.id AND versions.version = types.current_version
         WHERE types.id = $1 AND types.tenant_id = $2",
    )
    .bind(business_type_id)
    .bind(tenant_id)
    .fetch_optional(executor)
    .await?;
    row.map(postgres_business_type_from_row).transpose()
}

async fn list_business_types_for_asset(
    store: &PlatformStore,
    tenant_id: Uuid,
    asset_id: Uuid,
) -> Result<Vec<BusinessTypeDefinition>, BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            let rows = sqlx::query(
                "SELECT types.id, types.tenant_id, types.name, types.current_version,
                        types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
                 FROM business_types AS types
                 JOIN business_type_versions AS versions
                   ON versions.business_type_id = types.id AND versions.version = types.current_version
                 JOIN assets ON assets.id = ? AND assets.tenant_id = ?
                 WHERE types.tenant_id = ?
                   AND (json_array_length(versions.asset_profile_ids_json) = 0
                     OR EXISTS (
                       SELECT 1 FROM json_each(versions.asset_profile_ids_json)
                       WHERE json_each.value = assets.asset_profile_id
                     ))
                 ORDER BY types.name, types.id",
            )
            .bind(asset_id.to_string())
            .bind(tenant_id.to_string())
            .bind(tenant_id.to_string())
            .fetch_all(store.pool())
            .await?;
            rows.into_iter()
                .map(sqlite_business_type_from_row)
                .collect()
        }
        PlatformStore::Timescale(pool) => {
            let rows = sqlx::query(
                "SELECT types.id, types.tenant_id, types.name, types.current_version,
                        types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
                 FROM business_types AS types
                 JOIN business_type_versions AS versions
                   ON versions.business_type_id = types.id AND versions.version = types.current_version
                 JOIN assets ON assets.id = $1 AND assets.tenant_id = $2
                 WHERE types.tenant_id = $2
                   AND (jsonb_array_length(versions.asset_profile_ids_json) = 0
                     OR versions.asset_profile_ids_json @> jsonb_build_array(to_jsonb(assets.asset_profile_id)))
                 ORDER BY types.name, types.id",
            )
            .bind(asset_id)
            .bind(tenant_id)
            .fetch_all(pool)
            .await?;
            rows.into_iter()
                .map(postgres_business_type_from_row)
                .collect()
        }
    }
}

async fn list_business_types(
    store: &PlatformStore,
    tenant_id: Uuid,
) -> Result<Vec<BusinessTypeDefinition>, BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            let rows = sqlx::query(
                "SELECT types.id, types.tenant_id, types.name, types.current_version,
                        types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
                 FROM business_types AS types
                 JOIN business_type_versions AS versions
                   ON versions.business_type_id = types.id AND versions.version = types.current_version
                 WHERE types.tenant_id = ?
                 ORDER BY types.name, types.id",
            )
            .bind(tenant_id.to_string())
            .fetch_all(store.pool())
            .await?;
            rows.into_iter()
                .map(sqlite_business_type_from_row)
                .collect()
        }
        PlatformStore::Timescale(pool) => {
            let rows = sqlx::query(
                "SELECT types.id, types.tenant_id, types.name, types.current_version,
                        types.indexed_field_key, versions.fields_json, versions.asset_profile_ids_json
                 FROM business_types AS types
                 JOIN business_type_versions AS versions
                   ON versions.business_type_id = types.id AND versions.version = types.current_version
                 WHERE types.tenant_id = $1
                 ORDER BY types.name, types.id",
            )
            .bind(tenant_id)
            .fetch_all(pool)
            .await?;
            rows.into_iter()
                .map(postgres_business_type_from_row)
                .collect()
        }
    }
}

async fn delete_business_type(
    store: &PlatformStore,
    tenant_id: Uuid,
    business_type_id: Uuid,
) -> Result<(), BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            let ty = sqlite_business_type(&mut *transaction, tenant_id, business_type_id)
                .await?
                .ok_or(BusinessError::TypeNotFound)?;
            if let Some(key) = &ty.indexed_field_key {
                drop_sqlite_business_expression_index(&mut transaction, ty.tenant_id, ty.id, key)
                    .await?;
            }
            sqlx::query("DELETE FROM business_types WHERE id = ? AND tenant_id = ?")
                .bind(business_type_id.to_string())
                .bind(tenant_id.to_string())
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            let ty = postgres_business_type(&mut *transaction, tenant_id, business_type_id)
                .await?
                .ok_or(BusinessError::TypeNotFound)?;
            if let Some(key) = &ty.indexed_field_key {
                drop_postgres_business_expression_index(&mut transaction, ty.tenant_id, ty.id, key)
                    .await?;
            }
            sqlx::query("DELETE FROM business_types WHERE id = $1 AND tenant_id = $2")
                .bind(business_type_id)
                .bind(tenant_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
        }
    }
    Ok(())
}

async fn business_user_permissions(
    store: &PlatformStore,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<BusinessUserPermissions>, BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            let row = sqlx::query(
                "SELECT tenant_id, user_id, can_read, can_create, can_update, can_delete
                 FROM business_user_permissions WHERE tenant_id = ? AND user_id = ?",
            )
            .bind(tenant_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(store.pool())
            .await?;
            row.map(sqlite_business_permissions_from_row).transpose()
        }
        PlatformStore::Timescale(pool) => {
            let row = sqlx::query(
                "SELECT tenant_id, user_id, can_read, can_create, can_update, can_delete
                 FROM business_user_permissions WHERE tenant_id = $1 AND user_id = $2",
            )
            .bind(tenant_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
            row.map(postgres_business_permissions_from_row).transpose()
        }
    }
}

async fn set_business_user_permissions(
    store: &PlatformStore,
    permissions: BusinessUserPermissions,
) -> Result<BusinessUserPermissions, BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            sqlx::query(
                "INSERT INTO business_user_permissions (
                    tenant_id, user_id, can_read, can_create, can_update, can_delete, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT (tenant_id, user_id) DO UPDATE SET
                    can_read = excluded.can_read, can_create = excluded.can_create,
                    can_update = excluded.can_update, can_delete = excluded.can_delete,
                    updated_at = excluded.updated_at",
            )
            .bind(permissions.tenant_id.to_string())
            .bind(permissions.user_id.to_string())
            .bind(permissions.can_read)
            .bind(permissions.can_create)
            .bind(permissions.can_update)
            .bind(permissions.can_delete)
            .bind(Utc::now().to_rfc3339())
            .execute(store.pool())
            .await?;
        }
        PlatformStore::Timescale(pool) => {
            sqlx::query(
                "INSERT INTO business_user_permissions (
                    tenant_id, user_id, can_read, can_create, can_update, can_delete, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, now())
                 ON CONFLICT (tenant_id, user_id) DO UPDATE SET
                    can_read = excluded.can_read, can_create = excluded.can_create,
                    can_update = excluded.can_update, can_delete = excluded.can_delete,
                    updated_at = excluded.updated_at",
            )
            .bind(permissions.tenant_id)
            .bind(permissions.user_id)
            .bind(permissions.can_read)
            .bind(permissions.can_create)
            .bind(permissions.can_update)
            .bind(permissions.can_delete)
            .execute(pool)
            .await?;
        }
    }
    Ok(permissions)
}

async fn create_business_record(
    store: &PlatformStore,
    record: NewBusinessRecord,
) -> Result<BusinessRecord, BusinessError> {
    let id = Uuid::now_v7();
    let now = Utc::now();
    let (created_by_actor_kind, created_by_actor_id) = business_actor_storage(record.created_by);
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            let ty =
                sqlite_business_type(&mut *transaction, record.tenant_id, record.business_type_id)
                    .await?
                    .ok_or(BusinessError::TypeNotFound)?;
            validate_business_record_data(&ty, &record.data)?;
            ensure_sqlite_asset_is_allowed(&mut transaction, &ty, record.asset_id).await?;
            ensure_sqlite_business_actor(&mut transaction, record.tenant_id, record.created_by)
                .await?;
            let data_json = serde_json::to_string(&record.data)
                .map_err(|_| BusinessError::InvalidFieldSchema)?;
            sqlx::query(
                "INSERT INTO business_records (
                    id, tenant_id, asset_id, business_type_id, schema_version, data_json,
                    created_by_actor_kind, created_by_actor_id,
                    updated_by_actor_kind, updated_by_actor_id, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(record.tenant_id.to_string())
            .bind(record.asset_id.to_string())
            .bind(record.business_type_id.to_string())
            .bind(ty.version)
            .bind(data_json)
            .bind(created_by_actor_kind)
            .bind(created_by_actor_id.to_string())
            .bind(created_by_actor_kind)
            .bind(created_by_actor_id.to_string())
            .bind(now.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(BusinessRecord {
                id,
                tenant_id: record.tenant_id,
                asset_id: record.asset_id,
                business_type_id: record.business_type_id,
                schema_version: ty.version,
                data: record.data,
                created_by: record.created_by,
                updated_by: record.created_by,
                created_at: now,
                updated_at: now,
            })
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            let ty = postgres_business_type(
                &mut *transaction,
                record.tenant_id,
                record.business_type_id,
            )
            .await?
            .ok_or(BusinessError::TypeNotFound)?;
            validate_business_record_data(&ty, &record.data)?;
            ensure_postgres_asset_is_allowed(&mut transaction, &ty, record.asset_id).await?;
            ensure_postgres_business_actor(&mut transaction, record.tenant_id, record.created_by)
                .await?;
            sqlx::query(
                "INSERT INTO business_records (
                    id, tenant_id, asset_id, business_type_id, schema_version, data_json,
                    created_by_actor_kind, created_by_actor_id,
                    updated_by_actor_kind, updated_by_actor_id, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $7, $8, $9, $9)",
            )
            .bind(id)
            .bind(record.tenant_id)
            .bind(record.asset_id)
            .bind(record.business_type_id)
            .bind(ty.version)
            .bind(Json(record.data.clone()))
            .bind(created_by_actor_kind)
            .bind(created_by_actor_id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(BusinessRecord {
                id,
                tenant_id: record.tenant_id,
                asset_id: record.asset_id,
                business_type_id: record.business_type_id,
                schema_version: ty.version,
                data: record.data,
                created_by: record.created_by,
                updated_by: record.created_by,
                created_at: now,
                updated_at: now,
            })
        }
    }
}

async fn update_business_record(
    store: &PlatformStore,
    record: UpdateBusinessRecord,
) -> Result<BusinessRecord, BusinessError> {
    let now = Utc::now();
    let (updated_by_actor_kind, updated_by_actor_id) = business_actor_storage(record.updated_by);
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            let row = sqlx::query(
                "SELECT asset_id, business_type_id, schema_version,
                        created_by_actor_kind, created_by_actor_id, created_at
                 FROM business_records WHERE id = ? AND tenant_id = ?",
            )
            .bind(record.id.to_string())
            .bind(record.tenant_id.to_string())
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(BusinessError::RecordNotFound)?;
            let asset_id = parse_uuid(row.try_get::<String, _>("asset_id")?)?;
            let business_type_id = parse_uuid(row.try_get::<String, _>("business_type_id")?)?;
            let created_by = business_actor_from_storage(
                &row.try_get::<String, _>("created_by_actor_kind")?,
                parse_uuid(row.try_get::<String, _>("created_by_actor_id")?)?,
            )?;
            let created_at = parse_timestamp(&row.try_get::<String, _>("created_at")?)?;
            let ty = sqlite_business_type(&mut *transaction, record.tenant_id, business_type_id)
                .await?
                .ok_or(BusinessError::TypeNotFound)?;
            validate_business_record_data(&ty, &record.data)?;
            ensure_sqlite_business_actor(&mut transaction, record.tenant_id, record.updated_by)
                .await?;
            let data_json = serde_json::to_string(&record.data)
                .map_err(|_| BusinessError::InvalidFieldSchema)?;
            sqlx::query(
                "UPDATE business_records
                 SET data_json = ?, schema_version = ?, updated_by_actor_kind = ?,
                     updated_by_actor_id = ?, updated_at = ?
                 WHERE id = ? AND tenant_id = ?",
            )
            .bind(data_json)
            .bind(ty.version)
            .bind(updated_by_actor_kind)
            .bind(updated_by_actor_id.to_string())
            .bind(now.to_rfc3339())
            .bind(record.id.to_string())
            .bind(record.tenant_id.to_string())
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(BusinessRecord {
                id: record.id,
                tenant_id: record.tenant_id,
                asset_id,
                business_type_id,
                schema_version: ty.version,
                data: record.data,
                created_by,
                updated_by: record.updated_by,
                created_at,
                updated_at: now,
            })
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            let row = sqlx::query(
                "SELECT asset_id, business_type_id, created_by_actor_kind, created_by_actor_id,
                        created_at
                 FROM business_records WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
            )
            .bind(record.id)
            .bind(record.tenant_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(BusinessError::RecordNotFound)?;
            let asset_id: Uuid = row.try_get("asset_id")?;
            let business_type_id: Uuid = row.try_get("business_type_id")?;
            let created_by = business_actor_from_storage(
                row.try_get("created_by_actor_kind")?,
                row.try_get("created_by_actor_id")?,
            )?;
            let created_at: DateTime<Utc> = row.try_get("created_at")?;
            let ty = postgres_business_type(&mut *transaction, record.tenant_id, business_type_id)
                .await?
                .ok_or(BusinessError::TypeNotFound)?;
            validate_business_record_data(&ty, &record.data)?;
            ensure_postgres_business_actor(&mut transaction, record.tenant_id, record.updated_by)
                .await?;
            sqlx::query(
                "UPDATE business_records
                 SET data_json = $1, schema_version = $2, updated_by_actor_kind = $3,
                     updated_by_actor_id = $4, updated_at = $5
                 WHERE id = $6 AND tenant_id = $7",
            )
            .bind(Json(record.data.clone()))
            .bind(ty.version)
            .bind(updated_by_actor_kind)
            .bind(updated_by_actor_id)
            .bind(now)
            .bind(record.id)
            .bind(record.tenant_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(BusinessRecord {
                id: record.id,
                tenant_id: record.tenant_id,
                asset_id,
                business_type_id,
                schema_version: ty.version,
                data: record.data,
                created_by,
                updated_by: record.updated_by,
                created_at,
                updated_at: now,
            })
        }
    }
}

async fn delete_business_record(
    store: &PlatformStore,
    tenant_id: Uuid,
    record_id: Uuid,
) -> Result<(), BusinessError> {
    let affected = match store {
        PlatformStore::Sqlite(store) => {
            sqlx::query("DELETE FROM business_records WHERE id = ? AND tenant_id = ?")
                .bind(record_id.to_string())
                .bind(tenant_id.to_string())
                .execute(store.pool())
                .await?
                .rows_affected()
        }
        PlatformStore::Timescale(pool) => {
            sqlx::query("DELETE FROM business_records WHERE id = $1 AND tenant_id = $2")
                .bind(record_id)
                .bind(tenant_id)
                .execute(pool)
                .await?
                .rows_affected()
        }
    };
    if affected == 0 {
        return Err(BusinessError::RecordNotFound);
    }
    Ok(())
}

async fn query_business_records(
    query: &PlatformStore,
    request: BusinessRecordQuery,
) -> Result<BusinessRecordPage, BusinessError> {
    let ty = find_business_type(query, request.tenant_id, request.business_type_id)
        .await?
        .ok_or(BusinessError::TypeNotFound)?;
    let index_field = request
        .field_key
        .as_deref()
        .map(|key| {
            ty.fields
                .iter()
                .find(|field| field.key == key)
                .ok_or(BusinessError::IndexedFieldMissing)
        })
        .transpose()?
        .or_else(|| {
            ty.indexed_field_key
                .as_deref()
                .and_then(|key| ty.fields.iter().find(|field| field.key == key))
        });
    let values = match (&request.indexed_query, index_field) {
        (None, _) => Vec::new(),
        (Some(_), None) => return Err(BusinessError::IndexedFieldMissing),
        (Some(BusinessIndexedQuery::Equal(value)), Some(field)) => {
            vec![index_value(&field.field_type, value)?]
        }
        (Some(BusinessIndexedQuery::Range { from, to }), Some(field)) => from
            .iter()
            .chain(to.iter())
            .map(|value| index_value(&field.field_type, value))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let expression = index_field.map(|field| match query {
        PlatformStore::Sqlite(_) => sqlite_index_expression(&field.key, &field.field_type),
        PlatformStore::Timescale(_) => postgres_index_expression(&field.key, &field.field_type),
    });
    let max_records = i64::from(request.limit.clamp(1, 100));
    let mut sql = match query {
        PlatformStore::Sqlite(_) => String::from(
            "SELECT id, tenant_id, asset_id, business_type_id, schema_version, data_json,
                    created_by_actor_kind, created_by_actor_id,
                    updated_by_actor_kind, updated_by_actor_id, created_at, updated_at
             FROM business_records
             WHERE tenant_id = ? AND asset_id = ? AND business_type_id = ?",
        ),
        PlatformStore::Timescale(_) => String::from(
            "SELECT id, tenant_id, asset_id, business_type_id, schema_version, data_json,
                    created_by_actor_kind, created_by_actor_id,
                    updated_by_actor_kind, updated_by_actor_id, created_at, updated_at
             FROM business_records
             WHERE tenant_id = $1 AND asset_id = $2 AND business_type_id = $3",
        ),
    };
    match (&request.indexed_query, expression.as_deref()) {
        (Some(BusinessIndexedQuery::Equal(_)), Some(expression)) => match query {
            PlatformStore::Sqlite(_) => sql.push_str(&format!(" AND {expression} = ?")),
            PlatformStore::Timescale(_) => sql.push_str(&format!(" AND {expression} = $4")),
        },
        (Some(BusinessIndexedQuery::Range { from, to }), Some(expression)) => match query {
            PlatformStore::Sqlite(_) => {
                if from.is_some() {
                    sql.push_str(&format!(" AND {expression} >= ?"));
                }
                if to.is_some() {
                    sql.push_str(&format!(" AND {expression} <= ?"));
                }
            }
            PlatformStore::Timescale(_) => {
                let mut position = 4;
                if from.is_some() {
                    sql.push_str(&format!(" AND {expression} >= ${position}"));
                    position += 1;
                }
                if to.is_some() {
                    sql.push_str(&format!(" AND {expression} <= ${position}"));
                }
            }
        },
        _ => {}
    }
    match query {
        PlatformStore::Sqlite(_) => {
            if request.cursor.is_some() {
                sql.push_str(" AND id > ?");
            }
            sql.push_str(" ORDER BY created_at, id LIMIT ?");
        }
        PlatformStore::Timescale(_) => {
            let cursor_position = 4 + values.len();
            if request.cursor.is_some() {
                sql.push_str(&format!(" AND id > ${cursor_position}"));
                sql.push_str(&format!(
                    " ORDER BY created_at, id LIMIT ${}",
                    cursor_position + 1
                ));
            } else {
                sql.push_str(&format!(
                    " ORDER BY created_at, id LIMIT ${cursor_position}"
                ));
            }
        }
    }
    match query {
        PlatformStore::Sqlite(store) => {
            let mut statement = sqlx::query(AssertSqlSafe(sql))
                .bind(request.tenant_id.to_string())
                .bind(request.asset_id.to_string())
                .bind(request.business_type_id.to_string());
            for value in values {
                statement = bind_sqlite_index_value(statement, value);
            }
            if let Some(cursor) = request.cursor {
                statement = statement.bind(cursor.to_string());
            }
            let rows = statement
                .bind(max_records + 1)
                .fetch_all(store.pool())
                .await?;
            business_page(
                rows.into_iter()
                    .map(sqlite_business_record_from_row)
                    .collect::<Result<_, _>>()?,
                max_records,
            )
        }
        PlatformStore::Timescale(pool) => {
            let mut statement = sqlx::query(AssertSqlSafe(sql))
                .bind(request.tenant_id)
                .bind(request.asset_id)
                .bind(request.business_type_id);
            for value in values {
                statement = bind_postgres_index_value(statement, value);
            }
            if let Some(cursor) = request.cursor {
                statement = statement.bind(cursor);
            }
            let rows = statement.bind(max_records + 1).fetch_all(pool).await?;
            business_page(
                rows.into_iter()
                    .map(postgres_business_record_from_row)
                    .collect::<Result<_, _>>()?,
                max_records,
            )
        }
    }
}

fn business_page(
    mut records: Vec<BusinessRecord>,
    max_records: i64,
) -> Result<BusinessRecordPage, BusinessError> {
    let next_cursor = if records.len() > max_records as usize {
        records.pop();
        records.last().map(|record| record.id)
    } else {
        None
    };
    Ok(BusinessRecordPage {
        records,
        next_cursor,
    })
}

fn bind_sqlite_index_value<'q>(
    statement: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments>,
    value: IndexValue,
) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments> {
    match value {
        IndexValue::Integer(value) => statement.bind(value),
        IndexValue::Decimal(value) => statement.bind(value),
        IndexValue::Boolean(value) => statement.bind(value),
        IndexValue::Text(value) => statement.bind(value),
    }
}

fn bind_postgres_index_value<'q>(
    statement: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    value: IndexValue,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    match value {
        IndexValue::Integer(value) => statement.bind(value),
        IndexValue::Decimal(value) => statement.bind(value),
        IndexValue::Boolean(value) => statement.bind(value),
        IndexValue::Text(value) => statement.bind(value),
    }
}

fn business_expression_index_name(tenant_id: Uuid, type_id: Uuid, field_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tenant_id.as_bytes());
    hasher.update([0]);
    hasher.update(type_id.as_bytes());
    hasher.update([0]);
    hasher.update(field_key.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("br_{}_{}", type_id.simple(), &digest[..12])
}

#[allow(dead_code)]
async fn create_business_expression_index(
    store: &PlatformStore,
    ty: &BusinessTypeDefinition,
) -> Result<(), BusinessError> {
    ty.validate()?;
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            create_sqlite_business_expression_index(&mut transaction, ty).await?;
            transaction.commit().await?;
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            create_postgres_business_expression_index(&mut transaction, ty).await?;
            transaction.commit().await?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
async fn drop_business_expression_index(
    store: &PlatformStore,
    tenant_id: Uuid,
    type_id: Uuid,
    field_key: &str,
) -> Result<(), BusinessError> {
    match store {
        PlatformStore::Sqlite(store) => {
            let mut transaction = store.pool().begin_with("BEGIN IMMEDIATE").await?;
            drop_sqlite_business_expression_index(&mut transaction, tenant_id, type_id, field_key)
                .await?;
            transaction.commit().await?;
        }
        PlatformStore::Timescale(pool) => {
            let mut transaction = pool.begin().await?;
            drop_postgres_business_expression_index(
                &mut transaction,
                tenant_id,
                type_id,
                field_key,
            )
            .await?;
            transaction.commit().await?;
        }
    }
    Ok(())
}

async fn create_sqlite_business_expression_index(
    transaction: &mut Transaction<'_, Sqlite>,
    ty: &BusinessTypeDefinition,
) -> Result<(), BusinessError> {
    let Some(field_key) = &ty.indexed_field_key else {
        return Ok(());
    };
    let field = ty
        .fields
        .iter()
        .find(|field| field.key == *field_key)
        .ok_or(BusinessError::IndexedFieldMissing)?;
    let name = business_expression_index_name(ty.tenant_id, ty.id, field_key);
    let expression = sqlite_index_expression(field_key, &field.field_type);
    let statement = format!(
        "CREATE INDEX IF NOT EXISTS {name}
         ON business_records (tenant_id, business_type_id, {expression}, asset_id)"
    );
    sqlx::raw_sql(AssertSqlSafe(statement))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn create_postgres_business_expression_index(
    transaction: &mut Transaction<'_, Postgres>,
    ty: &BusinessTypeDefinition,
) -> Result<(), BusinessError> {
    let Some(field_key) = &ty.indexed_field_key else {
        return Ok(());
    };
    let field = ty
        .fields
        .iter()
        .find(|field| field.key == *field_key)
        .ok_or(BusinessError::IndexedFieldMissing)?;
    let statement = postgres_expression_index_statement(ty, field_key, &field.field_type);
    sqlx::raw_sql(AssertSqlSafe(statement))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn postgres_expression_index_statement(
    ty: &BusinessTypeDefinition,
    field_key: &str,
    field_type: &BusinessFieldType,
) -> String {
    let name = business_expression_index_name(ty.tenant_id, ty.id, field_key);
    let expression = postgres_index_expression(field_key, field_type);
    format!(
        "CREATE INDEX IF NOT EXISTS {name}
         ON business_records (tenant_id, business_type_id, {expression}, asset_id)
         WHERE tenant_id = '{}' AND business_type_id = '{}'",
        ty.tenant_id, ty.id,
    )
}

async fn drop_sqlite_business_expression_index(
    transaction: &mut Transaction<'_, Sqlite>,
    tenant_id: Uuid,
    type_id: Uuid,
    field_key: &str,
) -> Result<(), BusinessError> {
    let name = business_expression_index_name(tenant_id, type_id, field_key);
    sqlx::raw_sql(AssertSqlSafe(format!("DROP INDEX IF EXISTS {name}")))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn drop_postgres_business_expression_index(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    type_id: Uuid,
    field_key: &str,
) -> Result<(), BusinessError> {
    let name = business_expression_index_name(tenant_id, type_id, field_key);
    sqlx::raw_sql(AssertSqlSafe(format!("DROP INDEX IF EXISTS {name}")))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn sqlite_index_expression(field_key: &str, field_type: &BusinessFieldType) -> String {
    let value = format!("json_extract(data_json, '$.{field_key}')");
    match field_type {
        BusinessFieldType::Integer => format!("CAST({value} AS INTEGER)"),
        BusinessFieldType::Decimal => format!("CAST({value} AS REAL)"),
        BusinessFieldType::Date | BusinessFieldType::DateTime => format!("CAST({value} AS TEXT)"),
        BusinessFieldType::String
        | BusinessFieldType::Boolean
        | BusinessFieldType::Select { .. } => value,
    }
}

fn postgres_index_expression(field_key: &str, field_type: &BusinessFieldType) -> String {
    let value = format!("(data_json ->> '{field_key}')");
    match field_type {
        BusinessFieldType::Integer => format!("({value}::bigint)"),
        BusinessFieldType::Decimal => format!("({value}::double precision)"),
        BusinessFieldType::Date | BusinessFieldType::DateTime => value,
        BusinessFieldType::Boolean => format!("({value}::boolean)"),
        BusinessFieldType::String | BusinessFieldType::Select { .. } => value,
    }
}

async fn remove_sqlite_business_field(
    transaction: &mut Transaction<'_, Sqlite>,
    type_id: Uuid,
    schema_version: i64,
    field_key: &str,
) -> Result<(), BusinessError> {
    let path = format!("$.{field_key}");
    sqlx::query(
        "UPDATE business_records
         SET data_json = json_remove(data_json, ?), schema_version = ?, updated_at = ?
         WHERE business_type_id = ?",
    )
    .bind(path)
    .bind(schema_version)
    .bind(Utc::now().to_rfc3339())
    .bind(type_id.to_string())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn remove_postgres_business_field(
    transaction: &mut Transaction<'_, Postgres>,
    type_id: Uuid,
    schema_version: i64,
    field_key: &str,
) -> Result<(), BusinessError> {
    sqlx::query(
        "UPDATE business_records
         SET data_json = data_json - $1, schema_version = $2, updated_at = now()
         WHERE business_type_id = $3",
    )
    .bind(field_key)
    .bind(schema_version)
    .bind(type_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn delete_sqlite_profile_business_records(
    transaction: &mut Transaction<'_, Sqlite>,
    tenant_id: Uuid,
    type_id: Uuid,
    profile_id: Uuid,
) -> Result<(), BusinessError> {
    sqlx::query(
        "DELETE FROM business_records
         WHERE tenant_id = ? AND business_type_id = ?
           AND asset_id IN (
             SELECT id FROM assets WHERE tenant_id = ? AND asset_profile_id = ?
           )",
    )
    .bind(tenant_id.to_string())
    .bind(type_id.to_string())
    .bind(tenant_id.to_string())
    .bind(profile_id.to_string())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn delete_postgres_profile_business_records(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    type_id: Uuid,
    profile_id: Uuid,
) -> Result<(), BusinessError> {
    sqlx::query(
        "DELETE FROM business_records AS records
         USING assets
         WHERE records.asset_id = assets.id AND records.tenant_id = assets.tenant_id
           AND records.tenant_id = $1 AND records.business_type_id = $2
           AND assets.asset_profile_id = $3",
    )
    .bind(tenant_id)
    .bind(type_id)
    .bind(profile_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn ensure_sqlite_asset_is_allowed(
    transaction: &mut Transaction<'_, Sqlite>,
    ty: &BusinessTypeDefinition,
    asset_id: Uuid,
) -> Result<(), BusinessError> {
    let profile_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT asset_profile_id FROM assets WHERE id = ? AND tenant_id = ?",
    )
    .bind(asset_id.to_string())
    .bind(ty.tenant_id.to_string())
    .fetch_optional(&mut **transaction)
    .await?
    .flatten()
    .map(|value| parse_uuid(value))
    .transpose()?;
    if !ty.asset_profile_ids.is_empty()
        && !profile_id.is_some_and(|id| ty.asset_profile_ids.contains(&id))
    {
        return Err(BusinessError::TypeNotFound);
    }
    Ok(())
}

async fn ensure_postgres_asset_is_allowed(
    transaction: &mut Transaction<'_, Postgres>,
    ty: &BusinessTypeDefinition,
    asset_id: Uuid,
) -> Result<(), BusinessError> {
    let profile_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT asset_profile_id FROM assets WHERE id = $1 AND tenant_id = $2",
    )
    .bind(asset_id)
    .bind(ty.tenant_id)
    .fetch_optional(&mut **transaction)
    .await?
    .flatten();
    if !ty.asset_profile_ids.is_empty()
        && !profile_id.is_some_and(|id| ty.asset_profile_ids.contains(&id))
    {
        return Err(BusinessError::TypeNotFound);
    }
    Ok(())
}

fn business_actor_storage(actor: TenantActor) -> (&'static str, Uuid) {
    match actor {
        TenantActor::TenantUser(id) => ("tenant_user", id),
        TenantActor::TenantAccount(id) => ("tenant_account", id),
    }
}

fn business_actor_from_storage(kind: &str, id: Uuid) -> Result<TenantActor, BusinessError> {
    match kind {
        "tenant_user" => Ok(TenantActor::TenantUser(id)),
        "tenant_account" => Ok(TenantActor::TenantAccount(id)),
        _ => Err(BusinessError::InvalidStoredActor),
    }
}

async fn ensure_sqlite_business_actor(
    transaction: &mut Transaction<'_, Sqlite>,
    tenant_id: Uuid,
    actor: TenantActor,
) -> Result<(), BusinessError> {
    let (query, actor_id) = match actor {
        TenantActor::TenantUser(user_id) => (
            "SELECT 1 FROM users WHERE id = ? AND tenant_id = ?",
            user_id,
        ),
        TenantActor::TenantAccount(tenant_account_id) => (
            "SELECT 1 FROM tenant_accounts WHERE id = ? AND tenant_id = ?",
            tenant_account_id,
        ),
    };
    let exists = sqlx::query_scalar::<_, i32>(query)
        .bind(actor_id.to_string())
        .bind(tenant_id.to_string())
        .fetch_optional(&mut **transaction)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(BusinessError::ActorNotFound)
    }
}

async fn ensure_postgres_business_actor(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    actor: TenantActor,
) -> Result<(), BusinessError> {
    let (query, actor_id) = match actor {
        TenantActor::TenantUser(user_id) => (
            "SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2",
            user_id,
        ),
        TenantActor::TenantAccount(tenant_account_id) => (
            "SELECT 1 FROM tenant_accounts WHERE id = $1 AND tenant_id = $2",
            tenant_account_id,
        ),
    };
    let exists = sqlx::query_scalar::<_, i32>(query)
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_optional(&mut **transaction)
        .await?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(BusinessError::ActorNotFound)
    }
}

fn sqlite_business_type_from_row(row: SqliteRow) -> Result<BusinessTypeDefinition, BusinessError> {
    let fields = deserialize_fields(&row.try_get::<String, _>("fields_json")?)?;
    let definition = BusinessTypeDefinition {
        id: parse_uuid(row.try_get("id")?)?,
        tenant_id: parse_uuid(row.try_get("tenant_id")?)?,
        name: row.try_get("name")?,
        version: row.try_get("current_version")?,
        fields,
        asset_profile_ids: deserialize_profile_ids(
            &row.try_get::<String, _>("asset_profile_ids_json")?,
        )?,
        indexed_field_key: row.try_get("indexed_field_key")?,
    };
    definition.validate()?;
    Ok(definition)
}

fn postgres_business_type_from_row(row: PgRow) -> Result<BusinessTypeDefinition, BusinessError> {
    let fields = deserialize_json_fields(row.try_get::<Json<Value>, _>("fields_json")?.0)?;
    let definition = BusinessTypeDefinition {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        name: row.try_get("name")?,
        version: row.try_get("current_version")?,
        fields,
        asset_profile_ids: deserialize_json_profile_ids(
            row.try_get::<Json<Value>, _>("asset_profile_ids_json")?.0,
        )?,
        indexed_field_key: row.try_get("indexed_field_key")?,
    };
    definition.validate()?;
    Ok(definition)
}

fn sqlite_business_permissions_from_row(
    row: SqliteRow,
) -> Result<BusinessUserPermissions, BusinessError> {
    Ok(BusinessUserPermissions {
        tenant_id: parse_uuid(row.try_get("tenant_id")?)?,
        user_id: parse_uuid(row.try_get("user_id")?)?,
        can_read: row.try_get::<i64, _>("can_read")? != 0,
        can_create: row.try_get::<i64, _>("can_create")? != 0,
        can_update: row.try_get::<i64, _>("can_update")? != 0,
        can_delete: row.try_get::<i64, _>("can_delete")? != 0,
    })
}

fn postgres_business_permissions_from_row(
    row: PgRow,
) -> Result<BusinessUserPermissions, BusinessError> {
    Ok(BusinessUserPermissions {
        tenant_id: row.try_get("tenant_id")?,
        user_id: row.try_get("user_id")?,
        can_read: row.try_get("can_read")?,
        can_create: row.try_get("can_create")?,
        can_update: row.try_get("can_update")?,
        can_delete: row.try_get("can_delete")?,
    })
}

fn sqlite_business_record_from_row(row: SqliteRow) -> Result<BusinessRecord, BusinessError> {
    Ok(BusinessRecord {
        id: parse_uuid(row.try_get("id")?)?,
        tenant_id: parse_uuid(row.try_get("tenant_id")?)?,
        asset_id: parse_uuid(row.try_get("asset_id")?)?,
        business_type_id: parse_uuid(row.try_get("business_type_id")?)?,
        schema_version: row.try_get("schema_version")?,
        data: serde_json::from_str(&row.try_get::<String, _>("data_json")?)
            .map_err(|_| BusinessError::InvalidFieldSchema)?,
        created_by: business_actor_from_storage(
            &row.try_get::<String, _>("created_by_actor_kind")?,
            parse_uuid(row.try_get("created_by_actor_id")?)?,
        )?,
        updated_by: business_actor_from_storage(
            &row.try_get::<String, _>("updated_by_actor_kind")?,
            parse_uuid(row.try_get("updated_by_actor_id")?)?,
        )?,
        created_at: parse_timestamp(&row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_timestamp(&row.try_get::<String, _>("updated_at")?)?,
    })
}

fn postgres_business_record_from_row(row: PgRow) -> Result<BusinessRecord, BusinessError> {
    Ok(BusinessRecord {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        asset_id: row.try_get("asset_id")?,
        business_type_id: row.try_get("business_type_id")?,
        schema_version: row.try_get("schema_version")?,
        data: row.try_get::<Json<Value>, _>("data_json")?.0,
        created_by: business_actor_from_storage(
            row.try_get("created_by_actor_kind")?,
            row.try_get("created_by_actor_id")?,
        )?,
        updated_by: business_actor_from_storage(
            row.try_get("updated_by_actor_kind")?,
            row.try_get("updated_by_actor_id")?,
        )?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn serialize_fields(fields: &[BusinessFieldDefinition]) -> Result<String, BusinessError> {
    serde_json::to_string(
        &fields
            .iter()
            .map(StoredBusinessField::from)
            .collect::<Vec<_>>(),
    )
    .map_err(|_| BusinessError::InvalidFieldSchema)
}

fn json_fields(fields: &[BusinessFieldDefinition]) -> Result<Value, BusinessError> {
    serde_json::to_value(
        fields
            .iter()
            .map(StoredBusinessField::from)
            .collect::<Vec<_>>(),
    )
    .map_err(|_| BusinessError::InvalidFieldSchema)
}

fn deserialize_fields(value: &str) -> Result<Vec<BusinessFieldDefinition>, BusinessError> {
    let fields = serde_json::from_str::<Vec<StoredBusinessField>>(value)
        .map_err(|_| BusinessError::InvalidFieldSchema)?;
    fields
        .into_iter()
        .map(BusinessFieldDefinition::try_from)
        .collect()
}

fn deserialize_json_fields(value: Value) -> Result<Vec<BusinessFieldDefinition>, BusinessError> {
    let fields = serde_json::from_value::<Vec<StoredBusinessField>>(value)
        .map_err(|_| BusinessError::InvalidFieldSchema)?;
    fields
        .into_iter()
        .map(BusinessFieldDefinition::try_from)
        .collect()
}

fn serialize_profile_ids(ids: &[Uuid]) -> Result<String, BusinessError> {
    serde_json::to_string(&ids.iter().map(Uuid::to_string).collect::<Vec<_>>())
        .map_err(|_| BusinessError::InvalidFieldSchema)
}

fn json_profile_ids(ids: &[Uuid]) -> Result<Value, BusinessError> {
    serde_json::to_value(ids.iter().map(Uuid::to_string).collect::<Vec<_>>())
        .map_err(|_| BusinessError::InvalidFieldSchema)
}

fn deserialize_profile_ids(value: &str) -> Result<Vec<Uuid>, BusinessError> {
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|_| BusinessError::InvalidFieldSchema)?
        .into_iter()
        .map(parse_uuid)
        .collect()
}

fn deserialize_json_profile_ids(value: Value) -> Result<Vec<Uuid>, BusinessError> {
    serde_json::from_value::<Vec<String>>(value)
        .map_err(|_| BusinessError::InvalidFieldSchema)?
        .into_iter()
        .map(parse_uuid)
        .collect()
}

fn indexed_field_type(definition: &BusinessTypeDefinition) -> Option<String> {
    let field_key = definition.indexed_field_key.as_deref()?;
    definition
        .fields
        .iter()
        .find(|field| field.key == field_key)
        .map(|field| field_type_name(&field.field_type).to_owned())
}

fn field_type_name(field_type: &BusinessFieldType) -> &'static str {
    match field_type {
        BusinessFieldType::String => "string",
        BusinessFieldType::Integer => "integer",
        BusinessFieldType::Decimal => "decimal",
        BusinessFieldType::Boolean => "boolean",
        BusinessFieldType::Date => "date",
        BusinessFieldType::DateTime => "datetime",
        BusinessFieldType::Select { .. } => "select",
    }
}

fn index_value(field_type: &BusinessFieldType, value: &Value) -> Result<IndexValue, BusinessError> {
    match field_type {
        BusinessFieldType::Integer => value
            .as_i64()
            .map(IndexValue::Integer)
            .ok_or(BusinessError::InvalidFieldSchema),
        BusinessFieldType::Decimal => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(IndexValue::Decimal)
            .ok_or(BusinessError::InvalidFieldSchema),
        BusinessFieldType::Boolean => value
            .as_bool()
            .map(IndexValue::Boolean)
            .ok_or(BusinessError::InvalidFieldSchema),
        BusinessFieldType::Date => canonical_date(value)
            .map(IndexValue::Text)
            .ok_or(BusinessError::InvalidFieldSchema),
        BusinessFieldType::DateTime => canonical_datetime(value)
            .map(IndexValue::Text)
            .ok_or(BusinessError::InvalidFieldSchema),
        BusinessFieldType::String | BusinessFieldType::Select { .. } => value
            .as_str()
            .map(|value| IndexValue::Text(value.to_owned()))
            .ok_or(BusinessError::InvalidFieldSchema),
    }
}

fn validate_business_record_data(
    ty: &BusinessTypeDefinition,
    data: &Value,
) -> Result<(), BusinessError> {
    let values = data.as_object().ok_or(BusinessError::InvalidFieldSchema)?;
    if values
        .keys()
        .any(|key| !ty.fields.iter().any(|field| field.key == *key))
    {
        return Err(BusinessError::InvalidFieldSchema);
    }
    for field in &ty.fields {
        match values.get(&field.key) {
            None if field.required => return Err(BusinessError::InvalidFieldSchema),
            None => {}
            Some(value) if value_matches_business_field_type(value, &field.field_type) => {}
            Some(_) => return Err(BusinessError::InvalidFieldSchema),
        }
    }
    Ok(())
}

fn value_matches_business_field_type(value: &Value, field_type: &BusinessFieldType) -> bool {
    match field_type {
        BusinessFieldType::String => value.is_string(),
        BusinessFieldType::Integer => value.as_i64().is_some(),
        BusinessFieldType::Decimal => value.as_f64().is_some_and(f64::is_finite),
        BusinessFieldType::Boolean => value.is_boolean(),
        BusinessFieldType::Date => canonical_date(value).is_some(),
        BusinessFieldType::DateTime => canonical_datetime(value).is_some(),
        BusinessFieldType::Select { options } => value
            .as_str()
            .is_some_and(|value| options.iter().any(|option| option == value)),
    }
}

fn canonical_date(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    (parsed.format("%Y-%m-%d").to_string() == value).then(|| value.to_owned())
}

fn canonical_datetime(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    let parsed = DateTime::parse_from_rfc3339(value).ok()?;
    (parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Nanos, true)
        == value)
        .then(|| value.to_owned())
}

fn parse_uuid(value: String) -> Result<Uuid, BusinessError> {
    value.parse().map_err(|_| BusinessError::InvalidFieldSchema)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, BusinessError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .map(|timestamp| timestamp.and_utc())
        })
        .map_err(|_| BusinessError::InvalidFieldSchema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_typed_index_expressions_keep_immutable_casts_inside_one_index_element() {
        for (field_type, expected) in [
            (BusinessFieldType::Boolean, "boolean"),
            (BusinessFieldType::Integer, "bigint"),
            (BusinessFieldType::Decimal, "double precision"),
        ] {
            assert_eq!(
                postgres_index_expression("value", &field_type),
                format!("((data_json ->> 'value')::{expected})"),
            );
        }
        for field_type in [BusinessFieldType::Date, BusinessFieldType::DateTime] {
            assert_eq!(
                postgres_index_expression("value", &field_type),
                "(data_json ->> 'value')",
            );
        }
    }

    #[test]
    fn date_and_datetime_index_values_are_canonical_text() {
        assert!(matches!(
            index_value(&BusinessFieldType::Date, &serde_json::json!("2026-10-07")),
            Ok(IndexValue::Text(value)) if value == "2026-10-07"
        ));
        assert!(matches!(
            index_value(&BusinessFieldType::DateTime, &serde_json::json!("2026-10-07T00:00:00.000000000Z")),
            Ok(IndexValue::Text(value)) if value == "2026-10-07T00:00:00.000000000Z"
        ));
        assert!(
            index_value(
                &BusinessFieldType::DateTime,
                &serde_json::json!("2026-10-07T00:00:00Z"),
            )
            .is_err()
        );
        assert!(
            index_value(
                &BusinessFieldType::DateTime,
                &serde_json::json!("2026-10-07T00:00:00.1Z"),
            )
            .is_err()
        );
    }

    #[test]
    fn postgres_boolean_expression_index_is_tenant_type_scoped() {
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let ty = BusinessTypeDefinition {
            id: Uuid::from_u128(3),
            tenant_id: tenant_a,
            name: "Boolean records".to_owned(),
            version: 1,
            fields: vec![BusinessFieldDefinition {
                key: "approved".to_owned(),
                label: "Approved".to_owned(),
                field_type: BusinessFieldType::Boolean,
                required: true,
            }],
            asset_profile_ids: Vec::new(),
            indexed_field_key: Some("approved".to_owned()),
        };
        let statement =
            postgres_expression_index_statement(&ty, "approved", &BusinessFieldType::Boolean);
        assert!(statement.contains("((data_json ->> 'approved')::boolean)"));
        assert!(statement.contains(&format!(
            "WHERE tenant_id = '{}' AND business_type_id = '{}'",
            ty.tenant_id, ty.id
        )));
        assert_ne!(
            business_expression_index_name(tenant_a, ty.id, "approved"),
            business_expression_index_name(tenant_b, ty.id, "approved"),
        );
    }
}
