use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use chrono::{Duration, Utc};
use iot_api::{
    CoreAuthorizedCommandCreateRequest, CoreCommandCreateRequest, CoreCommandRecord,
    CoreCommandResponseRequest, CoreFacade, CoreFacadeError, CoreTelemetryPoint,
    CoreTelemetryQuery, TokenVault, public_v1_router,
};
use iot_nano_foundation::{DatabaseStorage, StorageConfiguration};
use iot_storage::{
    ApplicationKind, ApplicationRepository, BusinessFieldDefinition, BusinessFieldType,
    BusinessRepository, BusinessTypeDefinition, CreateManagementUser, ManagementUserRepository,
    NewApplication, NewBusinessRecord, NewTenant, NewTenantAccount, NewUserPersonalAccessToken,
    PlatformStore, TenantActor, TenantIdentityRepository, UserPersonalAccessTokenRepository,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

const APP_ID: &str = "business-api";
const CLIENT_ID: &str = "business-api-client";
const USER_TOKEN: &str = "business-api-user-token";
const DENIED_USER_TOKEN: &str = "business-api-denied-user-token";
const PAT_SECRET: &str = "iotpat_business-api-user-token";

struct NoopCoreFacade;

impl CoreFacade for NoopCoreFacade {
    fn create_command(
        &self,
        _request: CoreCommandCreateRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CoreCommandRecord, CoreFacadeError>> + Send + '_>> {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }

    fn create_authorized_command(
        &self,
        _request: CoreAuthorizedCommandCreateRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CoreCommandRecord, CoreFacadeError>> + Send + '_>> {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }

    fn get_command(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<CoreCommandRecord, CoreFacadeError>> + Send + '_>> {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }

    fn record_command_response(
        &self,
        _request: CoreCommandResponseRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), CoreFacadeError>> + Send + '_>> {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }

    fn telemetry(
        &self,
        _query: CoreTelemetryQuery,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<CoreTelemetryPoint>, CoreFacadeError>> + Send + '_>>
    {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }
}

struct BusinessFixture {
    _directory: tempfile::TempDir,
    app: Router,
    store: Arc<PlatformStore>,
    tenant_id: Uuid,
    owner_user_id: Uuid,
    asset_id: Uuid,
    business_type_id: Uuid,
}

impl BusinessFixture {
    async fn request(
        &self,
        method: Method,
        uri: String,
        token: &str,
        body: Value,
    ) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn create(&self, token: &str, date: &str) -> axum::response::Response {
        self.request(
            Method::POST,
            "/api/v1/business/records".to_owned(),
            token,
            harvest_payload(self.asset_id, self.business_type_id, date),
        )
        .await
    }

    async fn query(
        &self,
        token: &str,
        from: Option<Value>,
        to: Option<Value>,
    ) -> axum::response::Response {
        self.request(
            Method::POST,
            "/api/v1/business/records/query".to_owned(),
            token,
            json!({
                "asset_id": self.asset_id,
                "business_type_id": self.business_type_id,
                "indexed_equals": null,
                "indexed_from": from,
                "indexed_to": to,
                "limit": 100,
                "cursor": null,
            }),
        )
        .await
    }
}

async fn authorized_business_fixture() -> BusinessFixture {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(
        PlatformStore::open(&StorageConfiguration {
            storage: DatabaseStorage::Sqlite,
            database_url: None,
            sqlite_path: Some(directory.path().join("business-api.sqlite")),
            sqlite_busy_timeout_ms: 5_000,
        })
        .await
        .unwrap(),
    );
    let (tenant, _account) = TenantIdentityRepository::create_tenant_with_account(
        store.as_ref(),
        NewTenant {
            slug: "business-api".to_owned(),
            metadata: json!({}),
        },
        NewTenantAccount {
            password_hash: "unused".to_owned(),
        },
    )
    .await
    .unwrap();
    let owner = ManagementUserRepository::create_management_user(
        store.as_ref(),
        CreateManagementUser {
            tenant_id: tenant.id,
            username: "business-owner".to_owned(),
            password_hash: "unused".to_owned(),
        },
    )
    .await
    .unwrap();
    let denied_user = ManagementUserRepository::create_management_user(
        store.as_ref(),
        CreateManagementUser {
            tenant_id: tenant.id,
            username: "business-denied".to_owned(),
            password_hash: "unused".to_owned(),
        },
    )
    .await
    .unwrap();
    let asset_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assets (id, tenant_id, name, owner_user_id) VALUES (?, ?, 'Business asset', ?)",
    )
    .bind(asset_id.to_string())
    .bind(tenant.id.to_string())
    .bind(owner.id.to_string())
    .execute(store.sqlite_pool().unwrap())
    .await
    .unwrap();
    let business_type = BusinessRepository::upsert_business_type(
        store.as_ref(),
        BusinessTypeDefinition {
            id: Uuid::nil(),
            tenant_id: tenant.id,
            name: "Harvest".to_owned(),
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
            ],
            asset_profile_ids: Vec::new(),
            indexed_field_key: Some("ngay".to_owned()),
        },
    )
    .await
    .unwrap();
    ApplicationRepository::upsert_application(
        store.as_ref(),
        NewApplication {
            app_id: APP_ID.parse().unwrap(),
            tenant_id: tenant.id,
            kind: ApplicationKind::FullStack,
            launch_url: "https://business.example.test".to_owned(),
            client_id: CLIENT_ID.parse().unwrap(),
            redirect_uris: vec!["https://business.example.test/callback".parse().unwrap()],
            allowed_scopes: vec!["business:read".to_owned(), "business:write".to_owned()],
            enabled: true,
        },
    )
    .await
    .unwrap();
    issue_oauth_token(store.as_ref(), tenant.id, owner.id, USER_TOKEN).await;
    issue_oauth_token(store.as_ref(), tenant.id, denied_user.id, DENIED_USER_TOKEN).await;
    let app = public_v1_router::<()>(
        Arc::clone(&store),
        TokenVault::from_key_material("business-api-test-vault"),
        Arc::new(NoopCoreFacade),
    );
    BusinessFixture {
        _directory: directory,
        app,
        store,
        tenant_id: tenant.id,
        owner_user_id: owner.id,
        asset_id,
        business_type_id: business_type.id,
    }
}

async fn issue_oauth_token(store: &PlatformStore, tenant_id: Uuid, user_id: Uuid, token: &str) {
    let issued_at = Utc::now();
    sqlx::query(
        "INSERT INTO oauth_access_tokens (
            token_hash, app_id, tenant_id, user_id, scopes_json, issued_at, expires_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(format!("{:x}", Sha256::digest(token.as_bytes())))
    .bind(APP_ID)
    .bind(tenant_id.to_string())
    .bind(user_id.to_string())
    .bind(json!(["business:read", "business:write"]).to_string())
    .bind(issued_at.to_rfc3339())
    .bind((issued_at + Duration::hours(1)).to_rfc3339())
    .execute(store.sqlite_pool().unwrap())
    .await
    .unwrap();
}

async fn issue_pat(store: &PlatformStore, tenant_id: Uuid, user_id: Uuid) {
    UserPersonalAccessTokenRepository::rotate_user_personal_access_token(
        store,
        tenant_id,
        user_id,
        NewUserPersonalAccessToken {
            id: Uuid::now_v7(),
            name: "Business API".to_owned(),
            token_prefix: PAT_SECRET[..14].to_owned(),
            token_hash: format!("{:x}", Sha256::digest(PAT_SECRET.as_bytes())),
        },
        Utc::now(),
    )
    .await
    .unwrap();
}

fn harvest_payload(asset_id: Uuid, business_type_id: Uuid, date: &str) -> Value {
    json!({
        "asset_id": asset_id,
        "business_type_id": business_type_id,
        "data": { "ngay": date, "san_luong": 125.5 },
    })
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn oauth_user_with_asset_access_and_default_business_permissions_can_crud_records() {
    let fixture = authorized_business_fixture().await;
    let types = fixture
        .request(
            Method::GET,
            format!("/api/v1/business/types?asset_id={}", fixture.asset_id),
            USER_TOKEN,
            Value::Null,
        )
        .await;
    assert_eq!(types.status(), StatusCode::OK);
    assert_eq!(response_json(types).await.as_array().unwrap().len(), 1);
    let created = fixture.create(USER_TOKEN, "2026-10-07").await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let record_id = created["id"].as_str().unwrap();
    assert_eq!(created["created_by"]["kind"], "tenant_user");
    assert_eq!(
        created["created_by"]["id"],
        fixture.owner_user_id.to_string()
    );

    assert_eq!(
        fixture
            .query(
                USER_TOKEN,
                Some(json!("2026-10-01")),
                Some(json!("2026-10-31"))
            )
            .await
            .status(),
        StatusCode::OK
    );
    let updated = fixture
        .request(
            Method::PATCH,
            format!("/api/v1/business/records/{record_id}"),
            USER_TOKEN,
            json!({"data": {"ngay":"2026-10-08", "san_luong":126.5}}),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        fixture
            .request(
                Method::DELETE,
                format!("/api/v1/business/records/{record_id}"),
                USER_TOKEN,
                Value::Null,
            )
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn user_without_asset_access_cannot_read_or_write_the_assets_business_records() {
    let fixture = authorized_business_fixture().await;
    assert_eq!(
        fixture.query(DENIED_USER_TOKEN, None, None).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .create(DENIED_USER_TOKEN, "2026-10-07")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn disabled_business_update_permission_denies_patch_but_keeps_read_access() {
    let fixture = authorized_business_fixture().await;
    let record = BusinessRepository::create_business_record(
        fixture.store.as_ref(),
        NewBusinessRecord {
            tenant_id: fixture.tenant_id,
            asset_id: fixture.asset_id,
            business_type_id: fixture.business_type_id,
            data: json!({"ngay":"2026-10-07", "san_luong":125.5}),
            created_by: TenantActor::TenantUser(fixture.owner_user_id),
        },
    )
    .await
    .unwrap();
    let mut permissions = BusinessRepository::business_user_permissions(
        fixture.store.as_ref(),
        fixture.tenant_id,
        fixture.owner_user_id,
    )
    .await
    .unwrap()
    .unwrap();
    permissions.can_update = false;
    BusinessRepository::set_business_user_permissions(fixture.store.as_ref(), permissions)
        .await
        .unwrap();

    assert_eq!(
        fixture
            .request(
                Method::PATCH,
                format!("/api/v1/business/records/{}", record.id),
                USER_TOKEN,
                json!({"data": {"ngay":"2026-10-08", "san_luong":126.5}}),
            )
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture.query(USER_TOKEN, None, None).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn indexed_date_range_query_returns_only_matching_records() {
    let fixture = authorized_business_fixture().await;
    assert_eq!(
        fixture.create(USER_TOKEN, "2026-10-07").await.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        fixture.create(USER_TOKEN, "2026-11-01").await.status(),
        StatusCode::CREATED
    );
    let body = response_json(
        fixture
            .query(
                USER_TOKEN,
                Some(json!("2026-10-01")),
                Some(json!("2026-10-31")),
            )
            .await,
    )
    .await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn query_can_target_a_non_indexed_business_field() {
    let fixture = authorized_business_fixture().await;
    assert_eq!(
        fixture.create(USER_TOKEN, "2026-10-07").await.status(),
        StatusCode::CREATED
    );
    let response = fixture
        .request(
            Method::POST,
            "/api/v1/business/records/query".to_owned(),
            USER_TOKEN,
            json!({
                "asset_id": fixture.asset_id,
                "business_type_id": fixture.business_type_id,
                "field_key": "san_luong",
                "indexed_from": 125.5,
                "indexed_to": 125.5,
                "limit": 100,
                "cursor": null,
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn indexed_date_range_rejects_a_value_that_storage_would_reject() {
    let fixture = authorized_business_fixture().await;
    assert_eq!(
        fixture
            .query(USER_TOKEN, Some(json!(7)), None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn user_pat_can_crud_records_with_user_attribution() {
    let fixture = authorized_business_fixture().await;
    issue_pat(
        fixture.store.as_ref(),
        fixture.tenant_id,
        fixture.owner_user_id,
    )
    .await;
    let created = fixture.create(PAT_SECRET, "2026-10-07").await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let record_id = created["id"].as_str().unwrap();
    assert_eq!(created["created_by"]["kind"], "tenant_user");
    assert_eq!(
        created["created_by"]["id"],
        fixture.owner_user_id.to_string()
    );

    let updated = fixture
        .request(
            Method::PATCH,
            format!("/api/v1/business/records/{record_id}"),
            PAT_SECRET,
            json!({"data": {"ngay":"2026-10-08", "san_luong":126.5}}),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = response_json(updated).await;
    assert_eq!(updated["updated_by"]["kind"], "tenant_user");
    assert_eq!(
        fixture
            .request(
                Method::DELETE,
                format!("/api/v1/business/records/{record_id}"),
                PAT_SECRET,
                Value::Null,
            )
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
}
