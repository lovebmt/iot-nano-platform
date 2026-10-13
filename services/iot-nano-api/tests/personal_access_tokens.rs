use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::Utc;
use iot_api::{
    CoreAuthorizedCommandCreateRequest, CoreCommandCreateRequest, CoreCommandRecord,
    CoreCommandResponseRequest, CoreFacade, CoreFacadeError, CoreTelemetryPoint,
    CoreTelemetryQuery, TokenVault, public_v1_router, validate_bearer_access_token,
};
use iot_nano_foundation::{DatabaseStorage, StorageConfiguration};
use iot_storage::{
    AuditPrincipal, NewTenant, NewTenantAccount, NewUserPersonalAccessToken, PlatformStore,
    TenantIdentityRepository, UserPersonalAccessTokenRepository,
};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

const PAT_SECRET: &str = "iotpat_personal-access-token-secret";
const DISABLED_PAT_SECRET: &str = "iotpat_disabled-personal-access-token-secret";

struct NoopCoreFacade;

#[derive(Default)]
struct RecordingCoreFacade {
    authorized_commands: Mutex<Vec<CoreAuthorizedCommandCreateRequest>>,
}

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

impl CoreFacade for RecordingCoreFacade {
    fn create_command(
        &self,
        _request: CoreCommandCreateRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CoreCommandRecord, CoreFacadeError>> + Send + '_>> {
        Box::pin(async { Err(CoreFacadeError::NotFound) })
    }

    fn create_authorized_command(
        &self,
        request: CoreAuthorizedCommandCreateRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CoreCommandRecord, CoreFacadeError>> + Send + '_>> {
        let record = CoreCommandRecord {
            id: request.command.id,
            tenant_id: request.command.tenant_id,
            device_id: request.command.device_id.clone(),
            state: "queued".to_owned(),
            expires_at: request.command.expires_at,
            mode: request.command.mode,
            response: None,
            responded_at: None,
        };
        self.authorized_commands.lock().unwrap().push(request);
        Box::pin(async move { Ok(record) })
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

async fn public_router() -> (
    tempfile::TempDir,
    Arc<PlatformStore>,
    Uuid,
    Uuid,
    axum::Router,
) {
    public_router_with_core(Arc::new(NoopCoreFacade)).await
}

async fn public_router_with_core(
    core_facade: Arc<dyn CoreFacade>,
) -> (
    tempfile::TempDir,
    Arc<PlatformStore>,
    Uuid,
    Uuid,
    axum::Router,
) {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(
        PlatformStore::open(&StorageConfiguration {
            storage: DatabaseStorage::Sqlite,
            database_url: None,
            sqlite_path: Some(directory.path().join("personal-access-tokens.sqlite")),
            sqlite_busy_timeout_ms: 5_000,
        })
        .await
        .unwrap(),
    );
    let (tenant, _account) = TenantIdentityRepository::create_tenant_with_account(
        store.as_ref(),
        NewTenant {
            slug: "personal-access-token".to_owned(),
            metadata: serde_json::json!({}),
        },
        NewTenantAccount {
            password_hash: "unused".to_owned(),
        },
    )
    .await
    .unwrap();
    let user_id = seed_regular_user(store.as_ref(), tenant.id, "personal-access-token-user").await;
    let app = public_v1_router::<()>(
        Arc::clone(&store),
        TokenVault::from_key_material("personal-access-token-test-key-material"),
        core_facade,
    );
    (directory, store, tenant.id, user_id, app)
}

async fn seed_regular_user(store: &PlatformStore, tenant_id: Uuid, username: &str) -> Uuid {
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, username, password_hash, role, account_class)
         VALUES (?, ?, ?, 'unused', 'viewer', 'user')",
    )
    .bind(user_id.to_string())
    .bind(tenant_id.to_string())
    .bind(username)
    .execute(store.sqlite_pool().unwrap())
    .await
    .unwrap();
    user_id
}

async fn issue_pat(store: &PlatformStore, tenant_id: Uuid, account_id: Uuid, secret: &str) {
    UserPersonalAccessTokenRepository::rotate_user_personal_access_token(
        store,
        tenant_id,
        account_id,
        NewUserPersonalAccessToken {
            id: Uuid::now_v7(),
            name: "CI".to_owned(),
            token_prefix: secret[..14].to_owned(),
            token_hash: format!("{:x}", Sha256::digest(secret.as_bytes())),
        },
        Utc::now(),
    )
    .await
    .unwrap();
}

fn bearer_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn user_pat_authenticates_the_public_devices_resource() {
    let (_directory, store, tenant_id, user_id, app) = public_router().await;
    issue_pat(store.as_ref(), tenant_id, user_id, PAT_SECRET).await;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {PAT_SECRET}").parse().unwrap(),
    );
    let token = validate_bearer_access_token(store.as_ref(), &headers, Utc::now())
        .await
        .unwrap();
    assert_eq!(token.tenant_id, tenant_id);
    assert_eq!(token.user_id, Some(user_id));
    assert_eq!(
        token.scopes,
        [
            "assets:read",
            "assets:write",
            "devices:read",
            "devices:write",
            "telemetry:read",
            "alerts:read",
            "alerts:write",
            "commands:read",
            "commands:write",
            "rules:read",
            "rules:write",
            "business:read",
            "business:write",
        ]
        .map(str::to_owned),
    );

    let response = app
        .oneshot(bearer_request("/api/v1/devices", PAT_SECRET))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn revoked_user_pat_is_denied_but_tenant_account_status_does_not_change_user_access() {
    let (_directory, store, tenant_id, user_id, app) = public_router().await;
    issue_pat(store.as_ref(), tenant_id, user_id, DISABLED_PAT_SECRET).await;

    UserPersonalAccessTokenRepository::revoke_user_personal_access_token(
        store.as_ref(),
        tenant_id,
        user_id,
        Utc::now(),
    )
    .await
    .unwrap();
    let revoked = app
        .clone()
        .oneshot(bearer_request("/api/v1/devices", DISABLED_PAT_SECRET))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    issue_pat(store.as_ref(), tenant_id, user_id, PAT_SECRET).await;
    TenantIdentityRepository::disable_tenant_account(store.as_ref(), "personal-access-token")
        .await
        .unwrap();
    let disabled = app
        .oneshot(bearer_request("/api/v1/devices", PAT_SECRET))
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
}

#[tokio::test]
async fn public_router_does_not_expose_management_routes_to_a_pat() {
    let (_directory, store, tenant_id, user_id, app) = public_router().await;
    issue_pat(store.as_ref(), tenant_id, user_id, PAT_SECRET).await;

    let response = app
        .oneshot(bearer_request(
            "/api/v1/management/personal-access-token",
            PAT_SECRET,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_pat_cannot_bypass_user_capabilities() {
    let (_directory, store, tenant_id, user_id, app) = public_router().await;
    issue_pat(store.as_ref(), tenant_id, user_id, PAT_SECRET).await;

    let create_asset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/assets")
                .header(header::AUTHORIZATION, format!("Bearer {PAT_SECRET}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "PAT asset",
                        "asset_profile_id": null,
                        "parent_asset_id": null,
                        "metadata": {},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_asset.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn user_pat_commands_use_the_user_actor_and_exclude_authorization_writes() {
    let core = Arc::new(RecordingCoreFacade::default());
    let (_directory, store, tenant_id, user_id, app) = public_router_with_core(core.clone()).await;
    issue_pat(store.as_ref(), tenant_id, user_id, PAT_SECRET).await;
    seed_regular_user(store.as_ref(), tenant_id, "pat-recipient").await;
    sqlx::query(
        "INSERT INTO devices (device_id, tenant_id, display_name, owner_user_id)
         VALUES ('pat-command-device', ?, 'PAT command device', ?)",
    )
    .bind(tenant_id.to_string())
    .bind(user_id.to_string())
    .execute(store.sqlite_pool().unwrap())
    .await
    .unwrap();

    let command = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/devices/pat-command-device/commands")
                .header(header::AUTHORIZATION, format!("Bearer {PAT_SECRET}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "user-pat-command")
                .body(Body::from(
                    serde_json::json!({
                        "method": "sample_now",
                        "params": { "source": "pat" },
                        "mode": "one_way",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(command.status(), StatusCode::ACCEPTED);
    let commands = core.authorized_commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].actor,
        AuditPrincipal::User(user_id),
        "PAT command issuer must be the authenticated user"
    );
    drop(commands);

    let invitation = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/devices/pat-command-device/resource-invitations")
                .header(header::AUTHORIZATION, format!("Bearer {PAT_SECRET}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "username": "pat-recipient",
                        "permission": "manager",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invitation.status(), StatusCode::FORBIDDEN);
}
