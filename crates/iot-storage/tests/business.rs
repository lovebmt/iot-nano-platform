use iot_nano_foundation::{DatabaseStorage, StorageConfiguration};
use iot_storage::{
    BusinessError, BusinessFieldDefinition, BusinessFieldType, BusinessIndexedQuery,
    BusinessRecordQuery, BusinessRepository, BusinessTypeDefinition, CreateManagementUser,
    ManagementUserRepository, NewBusinessRecord, PlatformStore, TenantActor, UpdateBusinessRecord,
};
use serde_json::json;
use uuid::Uuid;

fn field(key: &str) -> BusinessFieldDefinition {
    BusinessFieldDefinition {
        key: key.to_owned(),
        label: key.to_owned(),
        field_type: BusinessFieldType::String,
        required: false,
    }
}

fn fields_with(key: &str, field_type: BusinessFieldType) -> Vec<BusinessFieldDefinition> {
    vec![BusinessFieldDefinition {
        key: key.to_owned(),
        label: key.to_owned(),
        field_type,
        required: false,
    }]
}

fn fields_with_select(key: &str) -> Vec<BusinessFieldDefinition> {
    fields_with(
        key,
        BusinessFieldType::Select {
            options: vec!["open".to_owned(), "closed".to_owned()],
        },
    )
}

#[test]
fn business_field_definition_rejects_an_invalid_key_and_duplicate_keys() {
    assert!(BusinessTypeDefinition::validate_fields(vec![field("ngay"), field("ngay")]).is_err());
    assert!(BusinessTypeDefinition::validate_fields(vec![field("Ngày")]).is_err());
}

#[test]
fn indexed_field_must_exist_and_use_an_indexable_type() {
    assert!(
        BusinessTypeDefinition::new(
            fields_with("note", BusinessFieldType::String),
            Some("missing"),
        )
        .is_err()
    );
    assert!(BusinessTypeDefinition::new(fields_with_select("status"), Some("status")).is_ok());
}

async fn sqlite_store() -> (tempfile::TempDir, PlatformStore, Uuid) {
    let directory = tempfile::tempdir().unwrap();
    let store = PlatformStore::open(&StorageConfiguration {
        storage: DatabaseStorage::Sqlite,
        database_url: None,
        sqlite_path: Some(directory.path().join("business.sqlite")),
        sqlite_busy_timeout_ms: 5_000,
    })
    .await
    .unwrap();
    let tenant_id = Uuid::now_v7();
    let pool = store.sqlite_pool().unwrap();
    sqlx::query("INSERT INTO tenants (id, slug, status) VALUES (?, ?, 'active')")
        .bind(tenant_id.to_string())
        .bind(format!("business-{tenant_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tenant_accounts (
            id, tenant_id, username, password_hash, status, credential_version
         ) VALUES (?1, ?2, ?1, 'unused', 'active', 1)",
    )
    .bind(tenant_id.to_string())
    .bind(tenant_id.to_string())
    .execute(pool)
    .await
    .unwrap();
    (directory, store, tenant_id)
}

async fn harvest_context(store: &PlatformStore, tenant_id: Uuid) -> (Uuid, Uuid, Uuid) {
    let pool = store.sqlite_pool().unwrap();
    let asset_profile_id = Uuid::now_v7();
    let asset_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO asset_profiles (id, tenant_id, name) VALUES (?, ?, 'Harvest')")
        .bind(asset_profile_id.to_string())
        .bind(tenant_id.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assets (id, tenant_id, name, asset_profile_id)
         VALUES (?, ?, 'Harvest asset', ?)",
    )
    .bind(asset_id.to_string())
    .bind(tenant_id.to_string())
    .bind(asset_profile_id.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, username, password_hash, role, account_class)
         VALUES (?, ?, ?, 'unused', 'viewer', 'user')",
    )
    .bind(user_id.to_string())
    .bind(tenant_id.to_string())
    .bind(format!("harvest-{user_id}"))
    .execute(pool)
    .await
    .unwrap();
    (asset_profile_id, asset_id, user_id)
}

fn harvest_type(tenant_id: Uuid, asset_profile_id: Uuid) -> BusinessTypeDefinition {
    BusinessTypeDefinition {
        id: Uuid::nil(),
        tenant_id,
        name: "Harvest record".to_owned(),
        version: 1,
        fields: vec![
            BusinessFieldDefinition {
                key: "ngay".to_owned(),
                label: "Ngày".to_owned(),
                field_type: BusinessFieldType::Date,
                required: true,
            },
            BusinessFieldDefinition {
                key: "san_luong".to_owned(),
                label: "Sản lượng".to_owned(),
                field_type: BusinessFieldType::Decimal,
                required: true,
            },
            BusinessFieldDefinition {
                key: "note".to_owned(),
                label: "Note".to_owned(),
                field_type: BusinessFieldType::String,
                required: false,
            },
        ],
        asset_profile_ids: vec![asset_profile_id],
        indexed_field_key: Some("ngay".to_owned()),
    }
}

async fn create_harvest_type(
    store: &PlatformStore,
    tenant_id: Uuid,
    asset_profile_id: Uuid,
) -> BusinessTypeDefinition {
    BusinessRepository::upsert_business_type(store, harvest_type(tenant_id, asset_profile_id))
        .await
        .unwrap()
}

#[tokio::test]
async fn sqlite_lists_all_business_types_for_a_tenant_in_name_order() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let mut zeta = harvest_type(tenant_id, Uuid::now_v7());
    zeta.name = "Zeta harvest".to_owned();
    let mut alpha = harvest_type(tenant_id, Uuid::now_v7());
    alpha.name = "Alpha harvest".to_owned();
    let zeta = BusinessRepository::upsert_business_type(&store, zeta)
        .await
        .unwrap();
    let alpha = BusinessRepository::upsert_business_type(&store, alpha)
        .await
        .unwrap();

    assert_eq!(
        BusinessRepository::list_business_types(&store, tenant_id)
            .await
            .unwrap(),
        vec![alpha, zeta],
    );
}

async fn insert_harvest_record(
    store: &PlatformStore,
    tenant_id: Uuid,
    asset_id: Uuid,
    type_id: Uuid,
    user_id: Uuid,
) -> Uuid {
    BusinessRepository::create_business_record(
        store,
        NewBusinessRecord {
            tenant_id,
            asset_id,
            business_type_id: type_id,
            data: json!({"ngay":"2026-10-07","san_luong":125.5,"note":"fresh"}),
            created_by: TenantActor::TenantUser(user_id),
        },
    )
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn sqlite_business_records_preserve_typed_user_and_tenant_account_attribution() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;

    let created = BusinessRepository::create_business_record(
        &store,
        NewBusinessRecord {
            tenant_id,
            asset_id,
            business_type_id: ty.id,
            data: json!({"ngay":"2026-10-07","san_luong":125.5,"note":"created by user"}),
            created_by: TenantActor::TenantUser(user_id),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.created_by, TenantActor::TenantUser(user_id));
    assert_eq!(created.updated_by, TenantActor::TenantUser(user_id));

    let updated = BusinessRepository::update_business_record(
        &store,
        UpdateBusinessRecord {
            id: created.id,
            tenant_id,
            data: json!({"ngay":"2026-10-08","san_luong":126.5,"note":"updated by account"}),
            updated_by: TenantActor::TenantAccount(tenant_id),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.created_by, TenantActor::TenantUser(user_id));
    assert_eq!(updated.updated_by, TenantActor::TenantAccount(tenant_id));

    let reread = records_for(&store, tenant_id, asset_id, ty.id, None)
        .await
        .pop()
        .unwrap();
    assert_eq!(reread.created_by, TenantActor::TenantUser(user_id));
    assert_eq!(reread.updated_by, TenantActor::TenantAccount(tenant_id));
}

#[tokio::test]
async fn sqlite_business_records_reject_cross_tenant_actor_attribution() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, _user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    let foreign_tenant_id = Uuid::now_v7();
    let foreign_account_id = Uuid::now_v7();
    let pool = store.sqlite_pool().unwrap();
    sqlx::query("INSERT INTO tenants (id, slug, status) VALUES (?, ?, 'active')")
        .bind(foreign_tenant_id.to_string())
        .bind(format!("foreign-{foreign_tenant_id}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tenant_accounts (
            id, tenant_id, username, password_hash, status, credential_version
         ) VALUES (?, ?, 'unused', 'unused', 'active', 1)",
    )
    .bind(foreign_account_id.to_string())
    .bind(foreign_tenant_id.to_string())
    .execute(pool)
    .await
    .unwrap();

    let error = BusinessRepository::create_business_record(
        &store,
        NewBusinessRecord {
            tenant_id,
            asset_id,
            business_type_id: ty.id,
            data: json!({"ngay":"2026-10-07","san_luong":125.5}),
            created_by: TenantActor::TenantAccount(foreign_account_id),
        },
    )
    .await
    .unwrap_err();

    assert!(matches!(error, BusinessError::ActorNotFound));
    assert!(
        records_for(&store, tenant_id, asset_id, ty.id, None)
            .await
            .is_empty()
    );
}

async fn records_for(
    store: &PlatformStore,
    tenant_id: Uuid,
    asset_id: Uuid,
    type_id: Uuid,
    indexed_query: Option<BusinessIndexedQuery>,
) -> Vec<iot_storage::BusinessRecord> {
    BusinessRepository::query_business_records(
        store,
        BusinessRecordQuery {
            tenant_id,
            asset_id,
            business_type_id: type_id,
            field_key: None,
            indexed_query,
            limit: 20,
            cursor: None,
        },
    )
    .await
    .unwrap()
    .records
}

#[tokio::test]
async fn sqlite_business_type_creates_one_expression_index_for_its_date_field() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, _asset_id, _user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    let indexes = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'business_records'",
    )
    .fetch_all(store.sqlite_pool().unwrap())
    .await
    .unwrap();
    assert_eq!(
        indexes
            .iter()
            .filter(|name| name.contains(&ty.id.simple().to_string()[..12]))
            .count(),
        1,
        "{indexes:?}"
    );
}

#[tokio::test]
async fn sqlite_business_range_query_uses_the_selected_date_field() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    insert_harvest_record(&store, tenant_id, asset_id, ty.id, user_id).await;
    assert_eq!(
        records_for(
            &store,
            tenant_id,
            asset_id,
            ty.id,
            Some(BusinessIndexedQuery::Range {
                from: Some(json!("2026-10-01")),
                to: Some(json!("2026-10-31")),
            }),
        )
        .await
        .len(),
        1
    );
}

#[tokio::test]
async fn deleting_a_field_removes_its_value_from_all_records() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    let record_id = insert_harvest_record(&store, tenant_id, asset_id, ty.id, user_id).await;
    ty.fields.retain(|field| field.key != "note");
    let updated = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    let record = records_for(&store, tenant_id, asset_id, updated.id, None)
        .await
        .into_iter()
        .find(|record| record.id == record_id)
        .unwrap();
    assert_eq!(record.data, json!({"ngay":"2026-10-07","san_luong":125.5}));
    assert_eq!(record.schema_version, updated.version);
}

#[tokio::test]
async fn removing_an_asset_profile_hard_deletes_matching_records() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    insert_harvest_record(&store, tenant_id, asset_id, ty.id, user_id).await;
    ty.asset_profile_ids.clear();
    let updated = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    assert!(
        records_for(&store, tenant_id, asset_id, updated.id, None)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn sqlite_management_users_receive_default_business_permissions() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let user = ManagementUserRepository::create_management_user(
        &store,
        CreateManagementUser {
            tenant_id,
            username: "business-operator".to_owned(),
            password_hash: "unused".to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        BusinessRepository::business_user_permissions(&store, tenant_id, user.id)
            .await
            .unwrap()
            .unwrap(),
        iot_storage::BusinessUserPermissions::all_enabled(tenant_id, user.id),
    );
}

#[tokio::test]
async fn sqlite_business_record_creation_rejects_unknown_missing_and_mistyped_values() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    for data in [
        json!({"ngay":"2026-10-07","san_luong":125.5,"unexpected":true}),
        json!({"ngay":"2026-10-07"}),
        json!({"ngay":7,"san_luong":125.5}),
    ] {
        assert!(
            BusinessRepository::create_business_record(
                &store,
                NewBusinessRecord {
                    tenant_id,
                    asset_id,
                    business_type_id: ty.id,
                    data,
                    created_by: TenantActor::TenantUser(user_id),
                },
            )
            .await
            .is_err()
        );
    }
    let mut selectable = ty;
    selectable.fields.push(BusinessFieldDefinition {
        key: "status".to_owned(),
        label: "Status".to_owned(),
        field_type: BusinessFieldType::Select {
            options: vec!["open".to_owned()],
        },
        required: false,
    });
    let selectable = BusinessRepository::upsert_business_type(&store, selectable)
        .await
        .unwrap();
    assert!(
        BusinessRepository::create_business_record(
            &store,
            NewBusinessRecord {
                tenant_id,
                asset_id,
                business_type_id: selectable.id,
                data: json!({"ngay":"2026-10-07","san_luong":125.5,"status":"closed"}),
                created_by: TenantActor::TenantUser(user_id),
            },
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn sqlite_business_record_update_validates_against_the_current_type_schema() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    let record_id = insert_harvest_record(&store, tenant_id, asset_id, ty.id, user_id).await;
    assert!(
        BusinessRepository::update_business_record(
            &store,
            UpdateBusinessRecord {
                id: record_id,
                tenant_id,
                data: json!({"ngay":"2026-10-07","san_luong":"125.5"}),
                updated_by: TenantActor::TenantUser(user_id),
            },
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn sqlite_empty_asset_profile_list_allows_records_and_lists_the_type() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    ty.asset_profile_ids.clear();
    let ty = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    assert_eq!(
        BusinessRepository::list_business_types_for_asset(&store, tenant_id, asset_id)
            .await
            .unwrap(),
        vec![ty.clone()]
    );
    insert_harvest_record(&store, tenant_id, asset_id, ty.id, user_id).await;
}

#[tokio::test]
async fn sqlite_business_field_cannot_change_type_after_delete_and_readd() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, _asset_id, _user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    ty.fields.retain(|field| field.key != "note");
    let mut ty = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    ty.fields.push(BusinessFieldDefinition {
        key: "note".to_owned(),
        label: "Replacement note".to_owned(),
        field_type: BusinessFieldType::Integer,
        required: false,
    });
    assert!(
        BusinessRepository::upsert_business_type(&store, ty)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sqlite_business_records_require_canonical_date_and_datetime_values() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    ty.fields.push(BusinessFieldDefinition {
        key: "observed_at".to_owned(),
        label: "Observed at".to_owned(),
        field_type: BusinessFieldType::DateTime,
        required: true,
    });
    let ty = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    for data in [
        json!({"ngay":"2026-10-7","san_luong":125.5,"observed_at":"2026-10-07T00:00:00.000000000Z"}),
        json!({"ngay":"2026-10-07","san_luong":125.5,"observed_at":"2026-10-07T00:00:00+00:00"}),
    ] {
        assert!(
            BusinessRepository::create_business_record(
                &store,
                NewBusinessRecord {
                    tenant_id,
                    asset_id,
                    business_type_id: ty.id,
                    data,
                    created_by: TenantActor::TenantUser(user_id),
                },
            )
            .await
            .is_err()
        );
    }
    assert!(
        BusinessRepository::create_business_record(
            &store,
            NewBusinessRecord {
                tenant_id,
                asset_id,
                business_type_id: ty.id,
                data: json!({"ngay":"2026-10-07","san_luong":125.5,"observed_at":"2026-10-07T00:00:00.000000000Z"}),
                created_by: TenantActor::TenantUser(user_id),
            },
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn sqlite_business_datetime_range_uses_fixed_width_utc_text() {
    let (_directory, store, tenant_id) = sqlite_store().await;
    let (asset_profile_id, asset_id, user_id) = harvest_context(&store, tenant_id).await;
    let mut ty = create_harvest_type(&store, tenant_id, asset_profile_id).await;
    ty.fields.push(BusinessFieldDefinition {
        key: "observed_at".to_owned(),
        label: "Observed at".to_owned(),
        field_type: BusinessFieldType::DateTime,
        required: true,
    });
    ty.indexed_field_key = Some("observed_at".to_owned());
    let ty = BusinessRepository::upsert_business_type(&store, ty)
        .await
        .unwrap();
    for observed_at in [
        "2026-10-07T00:00:00.000000000Z",
        "2026-10-07T00:00:00.100000000Z",
        "2026-10-07T00:00:00.300000000Z",
    ] {
        BusinessRepository::create_business_record(
            &store,
            NewBusinessRecord {
                tenant_id,
                asset_id,
                business_type_id: ty.id,
                data: json!({"ngay":"2026-10-07","san_luong":125.5,"observed_at":observed_at}),
                created_by: TenantActor::TenantUser(user_id),
            },
        )
        .await
        .unwrap();
    }
    let records = records_for(
        &store,
        tenant_id,
        asset_id,
        ty.id,
        Some(BusinessIndexedQuery::Range {
            from: Some(json!("2026-10-07T00:00:00.000000000Z")),
            to: Some(json!("2026-10-07T00:00:00.200000000Z")),
        }),
    )
    .await;
    assert_eq!(records.len(), 2);
    assert!(
        records.iter().any(|record| {
            record.data["observed_at"] == json!("2026-10-07T00:00:00.100000000Z")
        })
    );
}
