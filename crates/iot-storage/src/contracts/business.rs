use std::{collections::HashSet, future::Future, pin::Pin};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

use crate::PlatformStoreError;

use super::resources::TenantActor;

pub const BUSINESS_FIELD_KEY_PATTERN: &str = "[a-z][a-z0-9_]{0,63}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessFieldType {
    String,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Select { options: Vec<String> },
}

impl BusinessFieldType {
    pub const fn is_indexable(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessFieldDefinition {
    pub key: String,
    pub label: String,
    pub field_type: BusinessFieldType,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessTypeDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub version: i64,
    pub fields: Vec<BusinessFieldDefinition>,
    pub asset_profile_ids: Vec<Uuid>,
    pub indexed_field_key: Option<String>,
}

impl BusinessTypeDefinition {
    pub fn new(
        fields: Vec<BusinessFieldDefinition>,
        indexed_field_key: Option<&str>,
    ) -> Result<Self, BusinessError> {
        validate_business_type(&fields, indexed_field_key)?;
        Ok(Self {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            name: String::new(),
            version: 1,
            fields,
            asset_profile_ids: Vec::new(),
            indexed_field_key: indexed_field_key.map(str::to_owned),
        })
    }

    pub fn validate_fields(fields: Vec<BusinessFieldDefinition>) -> Result<(), BusinessError> {
        validate_business_type(&fields, None)
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        validate_business_type(&self.fields, self.indexed_field_key.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusinessRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub asset_id: Uuid,
    pub business_type_id: Uuid,
    pub schema_version: i64,
    pub data: serde_json::Value,
    pub created_by: TenantActor,
    pub updated_by: TenantActor,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewBusinessRecord {
    pub tenant_id: Uuid,
    pub asset_id: Uuid,
    pub business_type_id: Uuid,
    pub data: serde_json::Value,
    pub created_by: TenantActor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateBusinessRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub data: serde_json::Value,
    pub updated_by: TenantActor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessUserPermissions {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub can_read: bool,
    pub can_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
}

impl BusinessUserPermissions {
    pub const fn all_enabled(tenant_id: Uuid, user_id: Uuid) -> Self {
        Self {
            tenant_id,
            user_id,
            can_read: true,
            can_create: true,
            can_update: true,
            can_delete: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BusinessIndexedQuery {
    Equal(serde_json::Value),
    Range {
        from: Option<serde_json::Value>,
        to: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusinessRecordQuery {
    pub tenant_id: Uuid,
    pub asset_id: Uuid,
    pub business_type_id: Uuid,
    pub field_key: Option<String>,
    pub indexed_query: Option<BusinessIndexedQuery>,
    pub limit: u32,
    pub cursor: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusinessRecordPage {
    pub records: Vec<BusinessRecord>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum BusinessError {
    #[error("business fields must have unique keys matching {BUSINESS_FIELD_KEY_PATTERN}")]
    InvalidFieldSchema,
    #[error("the indexed business field does not exist")]
    IndexedFieldMissing,
    #[error("the indexed business field type cannot be indexed")]
    IndexedFieldNotIndexable,
    #[error("business type was not found")]
    TypeNotFound,
    #[error("business record was not found")]
    RecordNotFound,
    #[error("business user permissions were not found")]
    UserPermissionsNotFound,
    #[error("business actor was not found for the tenant")]
    ActorNotFound,
    #[error("stored business actor attribution is invalid")]
    InvalidStoredActor,
    #[error("business storage operation failed")]
    Storage {
        #[from]
        source: PlatformStoreError,
    },
}

pub fn validate_business_type(
    fields: &[BusinessFieldDefinition],
    indexed: Option<&str>,
) -> Result<(), BusinessError> {
    let keys = fields
        .iter()
        .map(|field| field.key.as_str())
        .collect::<HashSet<_>>();
    if keys.len() != fields.len() || fields.iter().any(|field| !valid_field_key(&field.key)) {
        return Err(BusinessError::InvalidFieldSchema);
    }
    if let Some(key) = indexed {
        let field = fields
            .iter()
            .find(|field| field.key == key)
            .ok_or(BusinessError::IndexedFieldMissing)?;
        if !field.field_type.is_indexable() {
            return Err(BusinessError::IndexedFieldNotIndexable);
        }
    }
    Ok(())
}

fn valid_field_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    matches!(bytes, [first, rest @ ..] if first.is_ascii_lowercase()
        && bytes.len() <= 64
        && rest.iter().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'))
}

pub trait BusinessRepository: Send + Sync {
    fn upsert_business_type<'a>(
        &'a self,
        definition: BusinessTypeDefinition,
    ) -> Pin<Box<dyn Future<Output = Result<BusinessTypeDefinition, BusinessError>> + Send + 'a>>;
    fn find_business_type<'a>(
        &'a self,
        tenant_id: Uuid,
        business_type_id: Uuid,
    ) -> Pin<
        Box<dyn Future<Output = Result<Option<BusinessTypeDefinition>, BusinessError>> + Send + 'a>,
    >;
    fn list_business_types<'a>(
        &'a self,
        tenant_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BusinessTypeDefinition>, BusinessError>> + Send + 'a>>;
    fn list_business_types_for_asset<'a>(
        &'a self,
        tenant_id: Uuid,
        asset_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<BusinessTypeDefinition>, BusinessError>> + Send + 'a>>;
    fn delete_business_type<'a>(
        &'a self,
        tenant_id: Uuid,
        business_type_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), BusinessError>> + Send + 'a>>;
    fn business_user_permissions<'a>(
        &'a self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<BusinessUserPermissions>, BusinessError>> + Send + 'a,
        >,
    >;
    fn set_business_user_permissions<'a>(
        &'a self,
        permissions: BusinessUserPermissions,
    ) -> Pin<Box<dyn Future<Output = Result<BusinessUserPermissions, BusinessError>> + Send + 'a>>;
    fn create_business_record<'a>(
        &'a self,
        record: NewBusinessRecord,
    ) -> Pin<Box<dyn Future<Output = Result<BusinessRecord, BusinessError>> + Send + 'a>>;
    fn update_business_record<'a>(
        &'a self,
        record: UpdateBusinessRecord,
    ) -> Pin<Box<dyn Future<Output = Result<BusinessRecord, BusinessError>> + Send + 'a>>;
    fn delete_business_record<'a>(
        &'a self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), BusinessError>> + Send + 'a>>;
    fn query_business_records<'a>(
        &'a self,
        query: BusinessRecordQuery,
    ) -> Pin<Box<dyn Future<Output = Result<BusinessRecordPage, BusinessError>> + Send + 'a>>;
}
