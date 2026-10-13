use std::{net::SocketAddr, sync::Arc};

use axum::{
    Extension,
    body::Body,
    extract::ConnectInfo,
    http::{
        Request, StatusCode,
        header::{CONTENT_TYPE, COOKIE, SET_COOKIE},
    },
};
use iot_api::{TokenVault, hash_password};
use iot_nano_foundation::{DatabaseStorage, StorageConfiguration};
use iot_nano_monolith::ManagementSessionRouter;
use iot_storage::{NewTenant, NewTenantAccount, PlatformStore, TenantIdentityRepository};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

async fn management_router() -> (tempfile::TempDir, axum::Router) {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(
        PlatformStore::open(&StorageConfiguration {
            storage: DatabaseStorage::Sqlite,
            database_url: None,
            sqlite_path: Some(directory.path().join("platform.sqlite")),
            sqlite_busy_timeout_ms: 5_000,
        })
        .await
        .unwrap(),
    );
    let (tenant, _) = TenantIdentityRepository::create_tenant_with_account(
        &store,
        NewTenant {
            slug: "test".to_owned(),
            metadata: json!({}),
        },
        NewTenantAccount {
            password_hash: hash_password("TenantAccount@2026").unwrap(),
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, username, password_hash, role, account_class)
         VALUES (?, ?, 'viewer', ?, 'viewer', 'user')",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(tenant.id.to_string())
    .bind(hash_password("NanoView@1234").unwrap())
    .execute(store.sqlite_pool().unwrap())
    .await
    .unwrap();
    let management = ManagementSessionRouter::new(store, test_token_vault());
    let router = management
        .router
        .layer(Extension(ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            0,
        )))));
    (directory, router)
}

fn test_token_vault() -> TokenVault {
    TokenVault::from_key_material([11_u8; 32])
}

async fn login_cookie(router: &axum::Router, path: &str, body: Value) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

async fn tenant_account_cookie(router: &axum::Router) -> String {
    login_cookie(
        router,
        "/api/v1/tenant/auth/login",
        json!({ "tenant_slug": "test", "password": "TenantAccount@2026" }),
    )
    .await
}

async fn tenant_user_cookie(router: &axum::Router) -> String {
    login_cookie(
        router,
        "/api/v1/user/auth/login",
        json!({
            "tenant_slug": "test",
            "username": "viewer",
            "password": "NanoView@1234",
        }),
    )
    .await
}

async fn request(
    router: &axum::Router,
    method: &str,
    path: &str,
    cookie: &str,
    body: Value,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(CONTENT_TYPE, "application/json")
                .header(COOKIE, cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

fn harvest_type_payload() -> Value {
    json!({
        "name": "Harvest",
        "fields": [
            { "key": "ngay", "label": "Date", "type": "date", "required": true },
            { "key": "san_luong", "label": "Quantity", "type": "decimal", "required": true },
            { "key": "note", "label": "Note", "type": "string", "required": false }
        ],
        "asset_profile_ids": [],
        "indexed_field_key": "ngay"
    })
}

fn all_business_permissions() -> Value {
    json!({ "read": true, "create": true, "update": true, "delete": true })
}

#[tokio::test]
async fn tenant_account_can_crud_business_records_for_an_asset() {
    let (_directory, router) = management_router().await;
    let cookie = tenant_account_cookie(&router).await;
    let asset = request(
        &router,
        "POST",
        "/api/v1/management/assets",
        &cookie,
        json!({ "name": "Business asset", "parent_asset_id": null, "asset_profile_id": null, "metadata": {} }),
    )
    .await;
    assert_eq!(asset.status(), StatusCode::CREATED);
    let asset_id = response_json(asset).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let business_type = request(
        &router,
        "POST",
        "/api/v1/management/business/types",
        &cookie,
        harvest_type_payload(),
    )
    .await;
    assert_eq!(business_type.status(), StatusCode::CREATED);
    let type_id = response_json(business_type).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = request(
        &router,
        "POST",
        &format!("/api/v1/management/assets/{asset_id}/business/records"),
        &cookie,
        json!({ "business_type_id": type_id, "data": { "ngay": "2026-10-11", "san_luong": 1.5 } }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let record_id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let listed = request(
        &router,
        "GET",
        &format!(
            "/api/v1/management/assets/{asset_id}/business/records?business_type_id={type_id}"
        ),
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(response_json(listed).await.as_array().unwrap().len(), 1);
    let updated = request(
        &router,
        "PUT",
        &format!("/api/v1/management/assets/{asset_id}/business/records/{record_id}"),
        &cookie,
        json!({ "data": { "ngay": "2026-10-12", "san_luong": 2.5 } }),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let other_asset = request(
        &router,
        "POST",
        "/api/v1/management/assets",
        &cookie,
        json!({ "name": "Other asset", "parent_asset_id": null, "asset_profile_id": null, "metadata": {} }),
    )
    .await;
    let other_asset_id = response_json(other_asset).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let wrong_asset_delete = request(
        &router,
        "DELETE",
        &format!("/api/v1/management/assets/{other_asset_id}/business/records/{record_id}"),
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(wrong_asset_delete.status(), StatusCode::NOT_FOUND);
    let deleted = request(
        &router,
        "DELETE",
        &format!("/api/v1/management/assets/{asset_id}/business/records/{record_id}"),
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn tenant_account_can_create_business_type_and_select_one_indexed_field() {
    let (_directory, router) = management_router().await;
    let cookie = tenant_account_cookie(&router).await;

    let response = request(
        &router,
        "POST",
        "/api/v1/management/business/types",
        &cookie,
        harvest_type_payload(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response_json(response).await["indexed_field_key"], "ngay");
}

#[tokio::test]
async fn tenant_user_cannot_manage_business_types_or_user_business_permissions() {
    let (_directory, router) = management_router().await;
    let cookie = tenant_user_cookie(&router).await;

    let create = request(
        &router,
        "POST",
        "/api/v1/management/business/types",
        &cookie,
        harvest_type_payload(),
    )
    .await;
    let permissions = request(
        &router,
        "PUT",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        all_business_permissions(),
    )
    .await;

    assert_eq!(create.status(), StatusCode::FORBIDDEN);
    assert_eq!(permissions.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tenant_account_can_disable_only_business_update_for_a_user() {
    let (_directory, router) = management_router().await;
    let cookie = tenant_account_cookie(&router).await;

    let updated = request(
        &router,
        "PUT",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        json!({ "read": true, "create": true, "update": false, "delete": true }),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);

    let permissions = request(
        &router,
        "GET",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(permissions.status(), StatusCode::OK);
    assert_eq!(response_json(permissions).await["update"], false);
}

#[tokio::test]
async fn tenant_account_gets_default_business_permissions_for_legacy_user_and_put_upserts_them() {
    let (_directory, router) = management_router().await;
    let cookie = tenant_account_cookie(&router).await;

    let defaults = request(
        &router,
        "GET",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(defaults.status(), StatusCode::OK);
    assert_eq!(response_json(defaults).await, all_business_permissions());

    let updated = request(
        &router,
        "PUT",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        json!({ "read": true, "create": true, "update": false, "delete": true }),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);

    let persisted = request(
        &router,
        "GET",
        "/api/v1/management/business/users/viewer/permissions",
        &cookie,
        Value::Null,
    )
    .await;
    assert_eq!(persisted.status(), StatusCode::OK);
    assert_eq!(response_json(persisted).await["update"], false);
}

#[tokio::test]
async fn tenant_business_page_is_visible_only_to_tenant_accounts() {
    let (_directory, router) = management_router().await;
    let account = tenant_account_cookie(&router).await;
    let user = tenant_user_cookie(&router).await;

    let page = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/tenant/business")
                .header(COOKIE, account)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let denied = router
        .oneshot(
            Request::builder()
                .uri("/tenant/business")
                .header(COOKIE, user)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}
