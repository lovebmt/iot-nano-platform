use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    routing::{get, patch, post},
};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use iot_storage::{
    BusinessError, BusinessFieldDefinition, BusinessFieldType, BusinessIndexedQuery,
    BusinessRecord, BusinessRecordPage as StorageBusinessRecordPage, BusinessRecordQuery,
    BusinessRepository, BusinessTypeDefinition, BusinessUserPermissions, NewBusinessRecord,
    PlatformStore, PublicApiRepository, PublicPrincipal, ResourcePermission, TenantActor,
    UpdateBusinessRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::public_v1::{PublicApiContext, PublicApiError, authenticate, tenant_actor};

const DEFAULT_RECORD_LIMIT: u32 = 50;
const MAX_RECORD_LIMIT: u32 = 100;

pub(crate) fn router<S>(context: PublicApiContext) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/v1/business/types", get(list_business_types_for_asset))
        .route(
            "/api/v1/business/records/query",
            post(query_business_records),
        )
        .route("/api/v1/business/records", post(create_business_record))
        .route(
            "/api/v1/business/records/{record_id}",
            patch(update_business_record).delete(delete_business_record),
        )
        .layer(Extension(context))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessTypeAssetQuery {
    asset_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessRecordQueryRequest {
    asset_id: Uuid,
    business_type_id: Uuid,
    #[serde(default)]
    field_key: Option<String>,
    #[serde(default)]
    indexed_equals: Option<Value>,
    #[serde(default)]
    indexed_from: Option<Value>,
    #[serde(default)]
    indexed_to: Option<Value>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    cursor: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBusinessRecordRequest {
    asset_id: Uuid,
    business_type_id: Uuid,
    data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateBusinessRecordRequest {
    data: Value,
}

#[derive(Debug, Serialize)]
struct BusinessTypeResponse {
    id: Uuid,
    name: String,
    version: i64,
    fields: Vec<BusinessFieldResponse>,
    asset_profile_ids: Vec<Uuid>,
    indexed_field_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct BusinessFieldResponse {
    key: String,
    label: String,
    #[serde(rename = "type")]
    field_type: &'static str,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct BusinessActorResponse {
    kind: &'static str,
    id: Uuid,
}

#[derive(Debug, Serialize)]
struct BusinessRecordResponse {
    id: Uuid,
    asset_id: Uuid,
    business_type_id: Uuid,
    schema_version: i64,
    data: Value,
    created_by: BusinessActorResponse,
    updated_by: BusinessActorResponse,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct BusinessRecordPage {
    items: Vec<BusinessRecordResponse>,
    next_cursor: Option<Uuid>,
    has_more: bool,
}

async fn list_business_types_for_asset(
    Extension(context): Extension<PublicApiContext>,
    headers: HeaderMap,
    Query(query): Query<BusinessTypeAssetQuery>,
) -> Result<Json<Vec<BusinessTypeResponse>>, PublicApiError> {
    let (store, principal) = authenticate(&context, &headers, "business:read").await?;
    require_business_access(&store, &principal, query.asset_id, BusinessAction::Read).await?;
    let types = BusinessRepository::list_business_types_for_asset(
        store.as_ref(),
        principal.tenant_id,
        query.asset_id,
    )
    .await
    .map_err(business_error)?;
    Ok(Json(
        types.into_iter().map(business_type_response).collect(),
    ))
}

async fn query_business_records(
    Extension(context): Extension<PublicApiContext>,
    headers: HeaderMap,
    Json(request): Json<BusinessRecordQueryRequest>,
) -> Result<Json<BusinessRecordPage>, PublicApiError> {
    let (store, principal) = authenticate(&context, &headers, "business:read").await?;
    require_business_access(&store, &principal, request.asset_id, BusinessAction::Read).await?;
    let business_type = BusinessRepository::find_business_type(
        store.as_ref(),
        principal.tenant_id,
        request.business_type_id,
    )
    .await
    .map_err(business_error)?
    .ok_or(PublicApiError::NotFound)?;
    let indexed_query = indexed_query(&business_type, &request)?;
    let limit = request.limit.unwrap_or(DEFAULT_RECORD_LIMIT);
    if !(1..=MAX_RECORD_LIMIT).contains(&limit) {
        return Err(PublicApiError::BadRequest);
    }
    let page = BusinessRepository::query_business_records(
        store.as_ref(),
        BusinessRecordQuery {
            tenant_id: principal.tenant_id,
            asset_id: request.asset_id,
            business_type_id: request.business_type_id,
            field_key: request.field_key.clone(),
            indexed_query,
            limit,
            cursor: request.cursor,
        },
    )
    .await
    .map_err(business_error)?;
    Ok(Json(business_record_page(page)))
}

async fn create_business_record(
    Extension(context): Extension<PublicApiContext>,
    headers: HeaderMap,
    Json(request): Json<CreateBusinessRecordRequest>,
) -> Result<(StatusCode, Json<BusinessRecordResponse>), PublicApiError> {
    let (store, principal) = authenticate(&context, &headers, "business:write").await?;
    let actor =
        require_business_access(&store, &principal, request.asset_id, BusinessAction::Create)
            .await?;
    let record = BusinessRepository::create_business_record(
        store.as_ref(),
        NewBusinessRecord {
            tenant_id: principal.tenant_id,
            asset_id: request.asset_id,
            business_type_id: request.business_type_id,
            data: request.data,
            created_by: actor,
        },
    )
    .await
    .map_err(business_error)?;
    Ok((StatusCode::CREATED, Json(business_record_response(record))))
}

async fn update_business_record(
    Extension(context): Extension<PublicApiContext>,
    headers: HeaderMap,
    Path(record_id): Path<Uuid>,
    Json(request): Json<UpdateBusinessRecordRequest>,
) -> Result<Json<BusinessRecordResponse>, PublicApiError> {
    let (store, principal) = authenticate(&context, &headers, "business:write").await?;
    let existing = find_accessible_business_record(&store, &principal, record_id).await?;
    let actor = require_business_access(
        &store,
        &principal,
        existing.asset_id,
        BusinessAction::Update,
    )
    .await?;
    let record = BusinessRepository::update_business_record(
        store.as_ref(),
        UpdateBusinessRecord {
            id: record_id,
            tenant_id: principal.tenant_id,
            data: request.data,
            updated_by: actor,
        },
    )
    .await
    .map_err(business_error)?;
    Ok(Json(business_record_response(record)))
}

async fn delete_business_record(
    Extension(context): Extension<PublicApiContext>,
    headers: HeaderMap,
    Path(record_id): Path<Uuid>,
) -> Result<StatusCode, PublicApiError> {
    let (store, principal) = authenticate(&context, &headers, "business:write").await?;
    let existing = find_accessible_business_record(&store, &principal, record_id).await?;
    require_business_access(
        &store,
        &principal,
        existing.asset_id,
        BusinessAction::Delete,
    )
    .await?;
    BusinessRepository::delete_business_record(store.as_ref(), principal.tenant_id, record_id)
        .await
        .map_err(business_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Copy)]
enum BusinessAction {
    Read,
    Create,
    Update,
    Delete,
}

async fn require_business_access(
    store: &Arc<PlatformStore>,
    principal: &PublicPrincipal,
    asset_id: Uuid,
    action: BusinessAction,
) -> Result<TenantActor, PublicApiError> {
    let has_asset_access =
        PublicApiRepository::public_asset_permission(store.as_ref(), principal, asset_id)
            .await
            .map_err(|_| PublicApiError::Unavailable)?
            .is_some_and(|permission| permission.allows(ResourcePermission::Viewer));
    if !has_asset_access {
        return Err(PublicApiError::Forbidden);
    }
    let actor = tenant_actor(principal)?;
    if principal.tenant_account_id.is_some() {
        return Ok(actor);
    }
    let user_id = principal.user_id.ok_or(PublicApiError::Forbidden)?;
    let permissions =
        BusinessRepository::business_user_permissions(store.as_ref(), principal.tenant_id, user_id)
            .await
            .map_err(business_error)?
            .unwrap_or_else(|| BusinessUserPermissions::all_enabled(principal.tenant_id, user_id));
    let allowed = match action {
        BusinessAction::Read => permissions.can_read,
        BusinessAction::Create => permissions.can_create,
        BusinessAction::Update => permissions.can_update,
        BusinessAction::Delete => permissions.can_delete,
    };
    if allowed {
        Ok(actor)
    } else {
        Err(PublicApiError::Forbidden)
    }
}

async fn find_accessible_business_record(
    store: &Arc<PlatformStore>,
    principal: &PublicPrincipal,
    record_id: Uuid,
) -> Result<BusinessRecord, PublicApiError> {
    let mut asset_after = None;
    loop {
        let assets = PublicApiRepository::list_public_assets(
            store.as_ref(),
            principal,
            asset_after.as_deref(),
            MAX_RECORD_LIMIT,
        )
        .await
        .map_err(|_| PublicApiError::Unavailable)?;
        if assets.is_empty() {
            return Err(PublicApiError::NotFound);
        }
        asset_after = assets.last().map(|asset| asset.id.to_string());
        for asset in assets {
            let types = BusinessRepository::list_business_types_for_asset(
                store.as_ref(),
                principal.tenant_id,
                asset.id,
            )
            .await
            .map_err(business_error)?;
            for business_type in types {
                let mut cursor = None;
                loop {
                    let page = BusinessRepository::query_business_records(
                        store.as_ref(),
                        BusinessRecordQuery {
                            tenant_id: principal.tenant_id,
                            asset_id: asset.id,
                            business_type_id: business_type.id,
                            field_key: None,
                            indexed_query: None,
                            limit: MAX_RECORD_LIMIT,
                            cursor,
                        },
                    )
                    .await
                    .map_err(business_error)?;
                    if let Some(record) = page
                        .records
                        .into_iter()
                        .find(|record| record.id == record_id)
                    {
                        return Ok(record);
                    }
                    let Some(next_cursor) = page.next_cursor else {
                        break;
                    };
                    cursor = Some(next_cursor);
                }
            }
        }
    }
}

fn indexed_query(
    business_type: &BusinessTypeDefinition,
    request: &BusinessRecordQueryRequest,
) -> Result<Option<BusinessIndexedQuery>, PublicApiError> {
    let has_range = request.indexed_from.is_some() || request.indexed_to.is_some();
    if request.indexed_equals.is_some() && has_range {
        return Err(PublicApiError::BadRequest);
    }
    let Some(value) = request.indexed_equals.as_ref() else {
        if !has_range {
            return Ok(None);
        }
        let field = query_field(business_type, request.field_key.as_deref())?;
        if !matches!(
            field.field_type,
            BusinessFieldType::Integer
                | BusinessFieldType::Decimal
                | BusinessFieldType::Date
                | BusinessFieldType::DateTime
        ) || !request
            .indexed_from
            .iter()
            .chain(request.indexed_to.iter())
            .all(|value| matches_index_value(&field.field_type, value))
        {
            return Err(PublicApiError::BadRequest);
        }
        return Ok(Some(BusinessIndexedQuery::Range {
            from: request.indexed_from.clone(),
            to: request.indexed_to.clone(),
        }));
    };
    let field = query_field(business_type, request.field_key.as_deref())?;
    if !matches!(
        field.field_type,
        BusinessFieldType::String | BusinessFieldType::Boolean | BusinessFieldType::Select { .. }
    ) || !matches_index_value(&field.field_type, value)
    {
        return Err(PublicApiError::BadRequest);
    }
    Ok(Some(BusinessIndexedQuery::Equal(value.clone())))
}

fn query_field<'a>(
    business_type: &'a BusinessTypeDefinition,
    field_key: Option<&str>,
) -> Result<&'a BusinessFieldDefinition, PublicApiError> {
    field_key
        .or(business_type.indexed_field_key.as_deref())
        .and_then(|key| business_type.fields.iter().find(|field| field.key == key))
        .ok_or(PublicApiError::BadRequest)
}

fn matches_index_value(field_type: &BusinessFieldType, value: &Value) -> bool {
    match field_type {
        BusinessFieldType::Integer => value.as_i64().is_some(),
        BusinessFieldType::Decimal => value.as_f64().is_some_and(f64::is_finite),
        BusinessFieldType::Boolean => value.is_boolean(),
        BusinessFieldType::Date => value.as_str().is_some_and(is_canonical_date),
        BusinessFieldType::DateTime => value.as_str().is_some_and(is_canonical_datetime),
        BusinessFieldType::String | BusinessFieldType::Select { .. } => value.is_string(),
    }
}

fn is_canonical_date(value: &str) -> bool {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .is_ok_and(|parsed| parsed.format("%Y-%m-%d").to_string() == value)
}

fn is_canonical_datetime(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|parsed| {
        parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Nanos, true)
            == value
    })
}

fn business_type_response(definition: BusinessTypeDefinition) -> BusinessTypeResponse {
    BusinessTypeResponse {
        id: definition.id,
        name: definition.name,
        version: definition.version,
        fields: definition
            .fields
            .into_iter()
            .map(business_field_response)
            .collect(),
        asset_profile_ids: definition.asset_profile_ids,
        indexed_field_key: definition.indexed_field_key,
    }
}

fn business_field_response(field: BusinessFieldDefinition) -> BusinessFieldResponse {
    let (field_type, options) = match field.field_type {
        BusinessFieldType::String => ("string", None),
        BusinessFieldType::Integer => ("integer", None),
        BusinessFieldType::Decimal => ("decimal", None),
        BusinessFieldType::Boolean => ("boolean", None),
        BusinessFieldType::Date => ("date", None),
        BusinessFieldType::DateTime => ("datetime", None),
        BusinessFieldType::Select { options } => ("select", Some(options)),
    };
    BusinessFieldResponse {
        key: field.key,
        label: field.label,
        field_type,
        required: field.required,
        options,
    }
}

fn business_record_page(page: StorageBusinessRecordPage) -> BusinessRecordPage {
    BusinessRecordPage {
        has_more: page.next_cursor.is_some(),
        next_cursor: page.next_cursor,
        items: page
            .records
            .into_iter()
            .map(business_record_response)
            .collect(),
    }
}

fn business_record_response(record: BusinessRecord) -> BusinessRecordResponse {
    BusinessRecordResponse {
        id: record.id,
        asset_id: record.asset_id,
        business_type_id: record.business_type_id,
        schema_version: record.schema_version,
        data: record.data,
        created_by: business_actor_response(record.created_by),
        updated_by: business_actor_response(record.updated_by),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn business_actor_response(actor: TenantActor) -> BusinessActorResponse {
    match actor {
        TenantActor::TenantUser(id) => BusinessActorResponse {
            kind: "tenant_user",
            id,
        },
        TenantActor::TenantAccount(id) => BusinessActorResponse {
            kind: "tenant_account",
            id,
        },
    }
}

fn business_error(error: BusinessError) -> PublicApiError {
    match error {
        BusinessError::InvalidFieldSchema
        | BusinessError::IndexedFieldMissing
        | BusinessError::IndexedFieldNotIndexable => PublicApiError::BadRequest,
        BusinessError::TypeNotFound | BusinessError::RecordNotFound => PublicApiError::NotFound,
        BusinessError::UserPermissionsNotFound
        | BusinessError::ActorNotFound
        | BusinessError::InvalidStoredActor => PublicApiError::Forbidden,
        BusinessError::Storage { .. } => PublicApiError::Unavailable,
    }
}
