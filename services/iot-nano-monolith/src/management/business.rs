use super::*;

use iot_storage::{
    BusinessError, BusinessFieldDefinition, BusinessFieldType, BusinessRecord, BusinessRecordQuery,
    BusinessRepository, BusinessTypeDefinition, BusinessUserPermissions, NewBusinessRecord,
    TenantActor, UpdateBusinessRecord,
};
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessTypeRequest {
    name: String,
    fields: Vec<BusinessFieldRequest>,
    #[serde(default)]
    asset_profile_ids: Vec<Uuid>,
    #[serde(default)]
    indexed_field_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessFieldRequest {
    key: String,
    label: String,
    #[serde(rename = "type")]
    field_type: String,
    required: bool,
    #[serde(default)]
    options: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BusinessPermissionsRequest {
    read: bool,
    create: bool,
    update: bool,
    delete: bool,
}

#[derive(Deserialize)]
pub(super) struct AssetBusinessRecordsQuery {
    business_type_id: Uuid,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBusinessRecordRequest {
    business_type_id: Uuid,
    data: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateBusinessRecordRequest {
    data: Value,
}

#[derive(Serialize)]
pub(super) struct BusinessTypeResponse {
    id: Uuid,
    name: String,
    version: i64,
    fields: Vec<BusinessFieldResponse>,
    asset_profile_ids: Vec<Uuid>,
    indexed_field_key: Option<String>,
}

#[derive(Serialize)]
struct BusinessFieldResponse {
    key: String,
    label: String,
    #[serde(rename = "type")]
    field_type: &'static str,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Vec<String>>,
}

#[derive(Serialize)]
pub(super) struct BusinessPermissionsResponse {
    read: bool,
    create: bool,
    update: bool,
    delete: bool,
}

#[derive(Serialize)]
pub(super) struct BusinessRecordResponse {
    id: Uuid,
    asset_id: Uuid,
    business_type_id: Uuid,
    data: Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

pub(super) async fn list_asset_business_records(
    State(state): State<ManagementState>,
    Path(asset_id): Path<Uuid>,
    Query(query): Query<AssetBusinessRecordsQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<BusinessRecordResponse>>, ManagementSessionError> {
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let page = BusinessRepository::query_business_records(
        state.store.as_ref(),
        BusinessRecordQuery {
            tenant_id: tenant.tenant_id,
            asset_id,
            business_type_id: query.business_type_id,
            field_key: None,
            indexed_query: None,
            limit: 100,
            cursor: None,
        },
    )
    .await
    .map_err(business_error)?;
    Ok(Json(
        page.records
            .into_iter()
            .map(business_record_response)
            .collect(),
    ))
}

pub(super) async fn create_asset_business_record(
    State(state): State<ManagementState>,
    Path(asset_id): Path<Uuid>,
    request: Request,
) -> Result<(StatusCode, Json<BusinessRecordResponse>), ManagementSessionError> {
    let headers = request.headers().clone();
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let request: CreateBusinessRecordRequest = management_request_json(&state, request).await?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    let record = BusinessRepository::create_business_record(
        state.store.as_ref(),
        NewBusinessRecord {
            tenant_id: tenant.tenant_id,
            asset_id,
            business_type_id: request.business_type_id,
            data: request.data,
            created_by: TenantActor::TenantAccount(tenant.tenant_account_id),
        },
    )
    .await
    .map_err(business_error)?;
    Ok((StatusCode::CREATED, Json(business_record_response(record))))
}

pub(super) async fn update_asset_business_record(
    State(state): State<ManagementState>,
    Path((_asset_id, record_id)): Path<(Uuid, Uuid)>,
    request: Request,
) -> Result<Json<BusinessRecordResponse>, ManagementSessionError> {
    let headers = request.headers().clone();
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let request: UpdateBusinessRecordRequest = management_request_json(&state, request).await?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    require_asset_business_record(&state, tenant.tenant_id, _asset_id, record_id).await?;
    let record = BusinessRepository::update_business_record(
        state.store.as_ref(),
        UpdateBusinessRecord {
            id: record_id,
            tenant_id: tenant.tenant_id,
            data: request.data,
            updated_by: TenantActor::TenantAccount(tenant.tenant_account_id),
        },
    )
    .await
    .map_err(business_error)?;
    Ok(Json(business_record_response(record)))
}

pub(super) async fn delete_asset_business_record(
    State(state): State<ManagementState>,
    Path((_asset_id, record_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<StatusCode, ManagementSessionError> {
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    require_asset_business_record(&state, tenant.tenant_id, _asset_id, record_id).await?;
    BusinessRepository::delete_business_record(state.store.as_ref(), tenant.tenant_id, record_id)
        .await
        .map_err(business_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_business_types(
    State(state): State<ManagementState>,
    headers: HeaderMap,
) -> Result<Json<Vec<BusinessTypeResponse>>, ManagementSessionError> {
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    BusinessRepository::list_business_types(state.store.as_ref(), tenant.tenant_id)
        .await
        .map(|types| Json(types.into_iter().map(business_type_response).collect()))
        .map_err(business_error)
}

pub(super) async fn create_business_type(
    State(state): State<ManagementState>,
    request: Request,
) -> Result<(StatusCode, Json<BusinessTypeResponse>), ManagementSessionError> {
    let headers = request.headers().clone();
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let request: BusinessTypeRequest = management_request_json(&state, request).await?;
    let definition = business_type_definition(tenant.tenant_id, Uuid::nil(), request)?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    let definition = BusinessRepository::upsert_business_type(state.store.as_ref(), definition)
        .await
        .map_err(business_error)?;
    Ok((
        StatusCode::CREATED,
        Json(business_type_response(definition)),
    ))
}

pub(super) async fn update_business_type(
    State(state): State<ManagementState>,
    Path(type_id): Path<String>,
    request: Request,
) -> Result<Json<BusinessTypeResponse>, ManagementSessionError> {
    let headers = request.headers().clone();
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let type_id = Uuid::parse_str(&type_id).map_err(|_| ManagementSessionError::BadRequest)?;
    let request: BusinessTypeRequest = management_request_json(&state, request).await?;
    let definition = business_type_definition(tenant.tenant_id, type_id, request)?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    if BusinessRepository::find_business_type(state.store.as_ref(), tenant.tenant_id, type_id)
        .await
        .map_err(business_error)?
        .is_none()
    {
        return Err(ManagementSessionError::NotFound);
    }
    let definition = BusinessRepository::upsert_business_type(state.store.as_ref(), definition)
        .await
        .map_err(business_update_error)?;
    Ok(Json(business_type_response(definition)))
}

pub(super) async fn delete_business_type(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(type_id): Path<String>,
) -> Result<StatusCode, ManagementSessionError> {
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let type_id = Uuid::parse_str(&type_id).map_err(|_| ManagementSessionError::BadRequest)?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    BusinessRepository::delete_business_type(state.store.as_ref(), tenant.tenant_id, type_id)
        .await
        .map_err(business_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn get_business_permissions(
    State(state): State<ManagementState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<BusinessPermissionsResponse>, ManagementSessionError> {
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let user_id = tenant_business_user_id(&state, tenant.tenant_id, &username).await?;
    let permissions = BusinessRepository::business_user_permissions(
        state.store.as_ref(),
        tenant.tenant_id,
        user_id,
    )
    .await
    .map_err(business_error)?
    .unwrap_or_else(|| BusinessUserPermissions::all_enabled(tenant.tenant_id, user_id));
    Ok(Json(business_permissions_response(permissions)))
}

pub(super) async fn set_business_permissions(
    State(state): State<ManagementState>,
    Path(username): Path<String>,
    request: Request,
) -> Result<Json<BusinessPermissionsResponse>, ManagementSessionError> {
    let headers = request.headers().clone();
    let tenant = require_tenant_account(&state.session_verifier, &headers)?;
    let request: BusinessPermissionsRequest = management_request_json(&state, request).await?;
    let user_id = tenant_business_user_id(&state, tenant.tenant_id, &username).await?;
    let _lease = authorize_tenant_mutation(&state, &headers).await?;
    let permissions = BusinessRepository::set_business_user_permissions(
        state.store.as_ref(),
        BusinessUserPermissions {
            tenant_id: tenant.tenant_id,
            user_id,
            can_read: request.read,
            can_create: request.create,
            can_update: request.update,
            can_delete: request.delete,
        },
    )
    .await
    .map_err(business_error)?;
    Ok(Json(business_permissions_response(permissions)))
}

fn business_type_definition(
    tenant_id: Uuid,
    id: Uuid,
    request: BusinessTypeRequest,
) -> Result<BusinessTypeDefinition, ManagementSessionError> {
    let name = request.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ManagementSessionError::BadRequest);
    }
    let fields = request
        .fields
        .into_iter()
        .map(business_field_definition)
        .collect::<Result<Vec<_>, _>>()?;
    let definition = BusinessTypeDefinition {
        id,
        tenant_id,
        name: name.to_owned(),
        version: 1,
        fields,
        asset_profile_ids: request.asset_profile_ids,
        indexed_field_key: request.indexed_field_key,
    };
    definition
        .validate()
        .map_err(|_| ManagementSessionError::BadRequest)?;
    Ok(definition)
}

fn business_field_definition(
    request: BusinessFieldRequest,
) -> Result<BusinessFieldDefinition, ManagementSessionError> {
    let field_type = match request.field_type.as_str() {
        "string" => BusinessFieldType::String,
        "integer" => BusinessFieldType::Integer,
        "decimal" => BusinessFieldType::Decimal,
        "boolean" => BusinessFieldType::Boolean,
        "date" => BusinessFieldType::Date,
        "datetime" => BusinessFieldType::DateTime,
        "select" => BusinessFieldType::Select {
            options: request.options.unwrap_or_default(),
        },
        _ => return Err(ManagementSessionError::BadRequest),
    };
    Ok(BusinessFieldDefinition {
        key: request.key,
        label: request.label,
        field_type,
        required: request.required,
    })
}

pub(super) fn business_type_response(definition: BusinessTypeDefinition) -> BusinessTypeResponse {
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

pub(super) fn business_permissions_response(
    permissions: BusinessUserPermissions,
) -> BusinessPermissionsResponse {
    BusinessPermissionsResponse {
        read: permissions.can_read,
        create: permissions.can_create,
        update: permissions.can_update,
        delete: permissions.can_delete,
    }
}

fn business_record_response(record: BusinessRecord) -> BusinessRecordResponse {
    BusinessRecordResponse {
        id: record.id,
        asset_id: record.asset_id,
        business_type_id: record.business_type_id,
        data: record.data,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

async fn require_asset_business_record(
    state: &ManagementState,
    tenant_id: Uuid,
    asset_id: Uuid,
    record_id: Uuid,
) -> Result<(), ManagementSessionError> {
    let types = BusinessRepository::list_business_types_for_asset(
        state.store.as_ref(),
        tenant_id,
        asset_id,
    )
    .await
    .map_err(business_error)?;
    for business_type in types {
        let mut cursor = None;
        loop {
            let page = BusinessRepository::query_business_records(
                state.store.as_ref(),
                BusinessRecordQuery {
                    tenant_id,
                    asset_id,
                    business_type_id: business_type.id,
                    field_key: None,
                    indexed_query: None,
                    limit: 100,
                    cursor,
                },
            )
            .await
            .map_err(business_error)?;
            if page.records.iter().any(|record| record.id == record_id) {
                return Ok(());
            }
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            cursor = Some(next_cursor);
        }
    }
    Err(ManagementSessionError::NotFound)
}

async fn tenant_business_user_id(
    state: &ManagementState,
    tenant_id: Uuid,
    username: &str,
) -> Result<Uuid, ManagementSessionError> {
    ManagementUserRepository::list_management_users(state.store.as_ref(), tenant_id)
        .await
        .map_err(management_user_error)?
        .into_iter()
        .find(|user| user.username == username && user.account_class.as_str() == "user")
        .map(|user| user.id)
        .ok_or(ManagementSessionError::NotFound)
}

pub(super) fn business_error(error: BusinessError) -> ManagementSessionError {
    match error {
        BusinessError::InvalidFieldSchema
        | BusinessError::IndexedFieldMissing
        | BusinessError::IndexedFieldNotIndexable => ManagementSessionError::BadRequest,
        BusinessError::TypeNotFound
        | BusinessError::RecordNotFound
        | BusinessError::UserPermissionsNotFound => ManagementSessionError::NotFound,
        BusinessError::ActorNotFound | BusinessError::InvalidStoredActor => {
            ManagementSessionError::Forbidden
        }
        BusinessError::Storage { .. } => ManagementSessionError::Unavailable,
    }
}

fn business_update_error(error: BusinessError) -> ManagementSessionError {
    match error {
        BusinessError::InvalidFieldSchema => ManagementSessionError::Conflict,
        error => business_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_actor_attribution_errors_are_forbidden() {
        for error in [
            BusinessError::ActorNotFound,
            BusinessError::InvalidStoredActor,
        ] {
            assert!(matches!(
                business_error(error),
                ManagementSessionError::Forbidden
            ));
        }
    }
}
