# Platform Business Data Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add general, tenant-defined, asset-bound business records to IoT Nano Platform and expose them in Power Monitor without creating a physical data table for each business type.

**Architecture:** Platform owns all business data in the existing Platform DB. A fixed set of configuration tables describes logical business types, schema versions, and user permissions; every submitted row lives in the single `business_records` table as validated `data_json`. Each business type can nominate one field for an expression index on `business_records`, so date/number range queries are indexed while users still see a logical virtual table per business type.

**Tech Stack:** Rust, Axum, SQLx, SQLite JSON functions, PostgreSQL/Timescale JSONB expression indexes, Next.js 16, React 19, Vitest.

## Global Constraints

- Bump the fresh Platform schema from version `8` to `9`; this is a development hard cut with no backwards migration.
- Keep all business data in the existing Platform database: `platform.sqlite` for SQLite and the existing PostgreSQL/Timescale schema for server deployments.
- Do not create a physical record table per Tenant or business type; `business_records` is the sole record table.
- Tenant Account alone manages business types, Asset Profile scope, and user business permissions.
- Tenant Users start with all four global business permissions enabled: `read`, `create`, `update`, and `delete`.
- A user action requires both the business permission and existing Platform access to the target Asset.
- Field types are `string`, `integer`, `decimal`, `boolean`, `date`, `datetime`, and `select`; an existing field type cannot change.
- Each business type can select at most one indexed field. The key is schema-derived and never accepted as raw SQL.
- Deletion is hard deletion: record delete removes a row; removing an Asset Profile deletes matching records; deleting a field removes that key from all rows; deleting a business type removes its records, versions, permission configuration, and expression index.
- Power Monitor remains a BFF/UI; it does not create a separate business database.

---

## File structure

| Path | Responsibility |
|---|---|
| `crates/iot-storage/src/contracts/business.rs` | Public storage contracts and typed business data models. |
| `crates/iot-storage/src/domain/business.rs` | SQLite and Timescale implementations, validation, cascading changes, and dynamic expression-index lifecycle. |
| `crates/iot-storage/src/contracts/mod.rs`, `crates/iot-storage/src/domain/mod.rs`, `crates/iot-storage/src/lib.rs` | Register and re-export the business domain. |
| `crates/iot-storage/src/schema/sqlite.rs`, `crates/iot-storage/migrations/0001_platform.sql` | Fresh schema v9 and fixed business tables/indexes. |
| `crates/iot-storage/src/management/users.rs` | Create default business permissions atomically with a Tenant User. |
| `crates/iot-storage/tests/business.rs` | SQLite and Timescale storage contract tests. |
| `services/iot-nano-api/src/business.rs` | OAuth/PAT-protected `/api/v1/business/*` record API. |
| `services/iot-nano-api/src/public_v1.rs`, `services/iot-nano-api/src/lib.rs` | Mount business router and share public auth context/error handling. |
| `services/iot-nano-api/tests/business.rs` | Public API authorization, filter, and CRUD tests. |
| `services/iot-nano-monolith/src/management/business.rs` | Tenant Account-only management JSON handlers. |
| `services/iot-nano-monolith/src/management/mod.rs`, `services/iot-nano-monolith/src/management/operator_api.rs` | Mount management APIs and map business errors. |
| `services/iot-nano-monolith/src/management/routes/tenant.rs` | Tenant Console page routes and form actions. |
| `services/iot-nano-monolith/templates/platform_ui/tenant_navigation.html` | Add Business Data navigation. |
| `services/iot-nano-monolith/templates/platform_ui/tenant_business.html` | Manage business types, fields, profile scope, index selection, and user permissions. |
| `services/iot-nano-monolith/tests/management_business.rs` | Tenant Account management API/page tests. |
| `apps/powermonitor/lib/browser-api.ts` | Browser BFF types and calls for business types and records. |
| `apps/powermonitor/components/business-records-panel.tsx` | Asset-bound virtual-table list, indexed range filter, dynamic editor, and delete UX. |
| `apps/powermonitor/components/powermonitor-dashboard.tsx` | Render the business records panel for the selected Asset. |
| `apps/powermonitor/tests/business-records-panel.test.tsx`, `apps/powermonitor/tests/browser-api.test.ts` | Power Monitor behavior and request contract tests. |
| `docs/superpowers/specs/2026-10-07-platform-business-data-design.md` | Update approved design with the one-record-table expression-index decision. |

## API contracts

### Tenant management API

```text
GET    /api/v1/management/business/types
POST   /api/v1/management/business/types
PUT    /api/v1/management/business/types/{type_id}
DELETE /api/v1/management/business/types/{type_id}
GET    /api/v1/management/business/users/{username}/permissions
PUT    /api/v1/management/business/users/{username}/permissions
```

`POST` and `PUT` type payload:

```json
{
  "name": "Sản lượng",
  "fields": [
    { "key": "ngay", "label": "Ngày", "type": "date", "required": true },
    { "key": "san_luong", "label": "Sản lượng", "type": "decimal", "required": true },
    { "key": "note", "label": "Ghi chú", "type": "string", "required": false }
  ],
  "asset_profile_ids": ["asset-profile-id"],
  "indexed_field_key": "ngay"
}
```

User permission payload:

```json
{ "read": true, "create": true, "update": true, "delete": true }
```

### Public business API

```text
GET    /api/v1/business/types?asset_id={asset_id}
POST   /api/v1/business/records/query
POST   /api/v1/business/records
PATCH  /api/v1/business/records/{record_id}
DELETE /api/v1/business/records/{record_id}
```

The query endpoint only uses the configured indexed field for server-side
filtering:

```json
{
  "asset_id": "asset-id",
  "business_type_id": "type-id",
  "indexed_equals": null,
  "indexed_from": "2026-10-01",
  "indexed_to": "2026-10-31",
  "limit": 100,
  "cursor": null
}
```

`indexed_from` and `indexed_to` are accepted only for indexed `integer`,
`decimal`, `date`, or `datetime` fields. `indexed_equals` is used for indexed
`string`, `boolean`, or `select` fields. The equality filter cannot be sent
with a range pair.

Create/update payload:

```json
{
  "asset_id": "asset-id",
  "business_type_id": "type-id",
  "data": {
    "ngay": "2026-10-07",
    "san_luong": 125.5,
    "note": "Thu hoạch lô phía bắc"
  }
}
```

### Core Rust contracts

```rust
pub enum BusinessFieldType {
    String,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Select { options: Vec<String> },
}

pub struct BusinessFieldDefinition {
    pub key: String,
    pub label: String,
    pub field_type: BusinessFieldType,
    pub required: bool,
}

pub struct BusinessTypeDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub version: i64,
    pub fields: Vec<BusinessFieldDefinition>,
    pub asset_profile_ids: Vec<Uuid>,
    pub indexed_field_key: Option<String>,
}

pub struct BusinessRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub asset_id: Uuid,
    pub business_type_id: Uuid,
    pub schema_version: i64,
    pub data: serde_json::Value,
    pub created_by_user_id: Uuid,
    pub updated_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct BusinessUserPermissions {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub can_read: bool,
    pub can_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
}
```

## Tasks

### Task 1: Lock the final one-record-table design into the schema contract

**Files:**
- Modify: `docs/superpowers/specs/2026-10-07-platform-business-data-design.md`
- Modify: `crates/iot-storage/src/contracts/mod.rs`
- Create: `crates/iot-storage/src/contracts/business.rs`
- Modify: `crates/iot-storage/src/lib.rs`
- Test: `crates/iot-storage/tests/business.rs`

**Consumes:** approved business-data design.

**Produces:** typed business contracts used by storage, Platform APIs, management APIs, and Power Monitor payloads.

- [ ] **Step 1: Write failing contract tests**

```rust
#[test]
fn business_field_definition_rejects_an_invalid_key_and_duplicate_keys() {
    assert!(BusinessTypeDefinition::validate_fields(vec![field("ngay"), field("ngay")]).is_err());
    assert!(BusinessTypeDefinition::validate_fields(vec![field("Ngày")]).is_err());
}

#[test]
fn indexed_field_must_exist_and_use_an_indexable_type() {
    assert!(BusinessTypeDefinition::new(fields_with("note", BusinessFieldType::String), Some("missing")).is_err());
    assert!(BusinessTypeDefinition::new(fields_with_select("status"), Some("status")).is_ok());
}
```

- [ ] **Step 2: Run the contract test and verify it fails**

Run: `cargo test -p iot-storage --test business business_field_definition_rejects_an_invalid_key_and_duplicate_keys -- --exact`

Expected: fail because the business contract module does not exist.

- [ ] **Step 3: Implement the typed contract and validation**

```rust
pub const BUSINESS_FIELD_KEY_PATTERN: &str = "[a-z][a-z0-9_]{0,63}";

pub fn validate_business_type(fields: &[BusinessFieldDefinition], indexed: Option<&str>) -> Result<(), BusinessError> {
    let keys = fields.iter().map(|field| field.key.as_str()).collect::<HashSet<_>>();
    if keys.len() != fields.len() || fields.iter().any(|field| !valid_field_key(&field.key)) {
        return Err(BusinessError::InvalidFieldSchema);
    }
    if let Some(key) = indexed {
        let field = fields.iter().find(|field| field.key == key).ok_or(BusinessError::IndexedFieldMissing)?;
        if !field.field_type.is_indexable() { return Err(BusinessError::IndexedFieldNotIndexable); }
    }
    Ok(())
}
```

Define `BusinessRepository` methods for type lifecycle, permissions, records,
and indexed-range query. Re-export the types and repository from
`iot_storage`.

- [ ] **Step 4: Update the approved design**

Replace the proposed `business_record_values` alternative with the agreed
single `business_records` table and a single schema-selected expression index
per business type. State explicitly that configuration tables remain fixed
Platform tables and no per-type record table is created.

- [ ] **Step 5: Run the contract tests**

Run: `cargo test -p iot-storage --test business -- --nocapture`

Expected: all contract validation tests pass.

- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/specs/2026-10-07-platform-business-data-design.md \
  crates/iot-storage/src/contracts/business.rs \
  crates/iot-storage/src/contracts/mod.rs \
  crates/iot-storage/src/lib.rs \
  crates/iot-storage/tests/business.rs
git commit -m "feat(storage): define business data contracts"
```

### Task 2: Add fresh schema v9 and storage implementation

**Files:**
- Modify: `crates/iot-storage/src/schema/sqlite.rs`
- Modify: `crates/iot-storage/migrations/0001_platform.sql`
- Create: `crates/iot-storage/src/domain/business.rs`
- Modify: `crates/iot-storage/src/domain/mod.rs`
- Modify: `crates/iot-storage/src/management/users.rs`
- Modify: `crates/iot-storage/tests/migration_safety.rs`
- Test: `crates/iot-storage/tests/business.rs`

**Consumes:** `BusinessRepository` and typed contracts from Task 1.

**Produces:** SQLite and Timescale-backed business type, permission, and
record operations that enforce all cascade rules transactionally.

- [ ] **Step 1: Write failing SQLite storage tests**

```rust
#[tokio::test]
async fn sqlite_business_type_creates_one_expression_index_for_its_date_field() {
    let store = sqlite_store().await;
    let ty = create_harvest_type(&store).await;
    assert!(sqlite_index_names(&store).await.iter().any(|name| name.contains(&ty.id.to_string())));
}

#[tokio::test]
async fn sqlite_business_range_query_uses_the_selected_date_field() {
    let store = sqlite_store().await;
    let (tenant, asset, ty, user) = seed_harvest_context(&store).await;
    insert_record(&store, tenant, asset, ty, user, json!({"ngay":"2026-10-07","san_luong":125.5})).await;
    assert_eq!(query_date_range(&store, tenant, asset, ty, "2026-10-01", "2026-10-31").await.len(), 1);
}

#[tokio::test]
async fn deleting_a_field_removes_its_value_from_all_records() {
    let (store, ty, record) = seeded_harvest_record().await;
    remove_business_field(&store, ty.id, "note").await.unwrap();
    assert_eq!(load_business_record(&store, record.id).await.unwrap().data, json!({"ngay":"2026-10-07","san_luong":125.5}));
}

#[tokio::test]
async fn removing_an_asset_profile_hard_deletes_matching_records() {
    let (store, ty, asset_profile_id, asset_id) = seeded_profile_scoped_record().await;
    remove_business_asset_profile(&store, ty.id, asset_profile_id).await.unwrap();
    assert!(list_business_records(&store, ty.tenant_id, asset_id, ty.id).await.unwrap().is_empty());
}
```

- [ ] **Step 2: Run the SQLite tests and verify they fail**

Run: `cargo test -p iot-storage --test business sqlite_business -- --nocapture`

Expected: fail because the v9 tables and repository implementation do not yet exist.

- [ ] **Step 3: Create the fixed tables and schema version 9**

Create the following in both fresh schemas:

```sql
business_types(id, tenant_id, name, current_version, indexed_field_key, indexed_field_type, created_at, updated_at)
business_type_versions(id, business_type_id, version, fields_json, asset_profile_ids_json, created_at)
business_records(id, tenant_id, asset_id, business_type_id, schema_version, data_json, created_by_user_id, updated_by_user_id, created_at, updated_at)
business_user_permissions(tenant_id, user_id, can_read, can_create, can_update, can_delete, updated_at)
```

Add fixed lookup indexes for `(tenant_id, business_type_id, asset_id,
created_at)` and foreign-key/cascade constraints. Set fresh schema version to
`9` in both SQLite and PostgreSQL definitions.

- [ ] **Step 4: Implement safe dynamic expression-index lifecycle**

Implement backend-only SQL construction from the validated field key and
field type. Use a stable hash of `(tenant_id, business_type_id, field_key)` in
the SQL index name. Do not interpolate user-provided SQL.

```rust
fn business_expression_index_name(type_id: Uuid, field_key: &str) -> String;
fn create_business_expression_index(store: &PlatformStore, ty: &BusinessTypeDefinition) -> Result<(), BusinessError>;
fn drop_business_expression_index(store: &PlatformStore, type_id: Uuid, field_key: &str) -> Result<(), BusinessError>;
```

SQLite uses `json_extract(data_json, '$.<validated_key>')`. PostgreSQL uses
the matching JSONB expression and casts it for `integer`, `decimal`, `date`,
or `datetime`; `string`, `boolean`, and `select` use equality expressions.
The composite index begins with `tenant_id` and
`business_type_id` and then the expression; `asset_id` is the final key.

- [ ] **Step 5: Implement repository writes and destructive rules**

In a transaction:

- create/update a type by validating schema and writing a new version;
- reject a changed field type;
- add field without touching existing record values;
- remove field by `json_remove` in SQLite and `data_json - field_key` in
  PostgreSQL, then advance record schema version;
- remove Asset Profile by deleting records joined to tenant Assets with that
  profile;
- delete a type by dropping its expression index then deleting versions,
  records, and permissions;
- create a default all-true permission row in
  `create_management_user`'s existing transaction.

- [ ] **Step 6: Add Timescale parity tests**

Run the same contract through `Backend::Timescale`, asserting typed indexed
range results and every hard-delete rule. Extend the existing test harness
pattern in `crates/iot-storage/tests/backend_contract.rs`.

- [ ] **Step 7: Run storage verification**

Run:

```bash
cargo test -p iot-storage --test business -- --nocapture
cargo test -p iot-storage --test migration_safety -- --nocapture
```

Expected: SQLite passes locally; Timescale tests pass when the existing
`IOT_NANO_TIMESCALE_TEST_URL` test environment is configured.

- [ ] **Step 8: Commit**

```bash
git add crates/iot-storage/src/domain/business.rs crates/iot-storage/src/domain/mod.rs \
  crates/iot-storage/src/schema/sqlite.rs crates/iot-storage/migrations/0001_platform.sql \
  crates/iot-storage/src/management/users.rs crates/iot-storage/tests/business.rs \
  crates/iot-storage/tests/migration_safety.rs
git commit -m "feat(storage): persist indexed business records"
```

### Task 3: Expose Tenant Account management APIs and console

**Files:**
- Create: `services/iot-nano-monolith/src/management/business.rs`
- Modify: `services/iot-nano-monolith/src/management/mod.rs`
- Modify: `services/iot-nano-monolith/src/management/operator_api.rs`
- Modify: `services/iot-nano-monolith/src/management/routes/tenant.rs`
- Modify: `services/iot-nano-monolith/templates/platform_ui/tenant_navigation.html`
- Create: `services/iot-nano-monolith/templates/platform_ui/tenant_business.html`
- Modify: `services/iot-nano-monolith/templates/platform_ui/tenant_users.html`
- Test: `services/iot-nano-monolith/tests/management_business.rs`
- Test: `services/iot-nano-monolith/tests/platform_ui_templates.rs`

**Consumes:** business storage APIs from Tasks 1–2 and existing
`require_tenant_account` / `authorize_tenant_mutation` management patterns.

**Produces:** Tenant Account-only type and permission management endpoints
plus the Tenant Console screens that operate them.

- [ ] **Step 1: Write failing management API tests**

```rust
#[tokio::test]
async fn tenant_account_can_create_business_type_and_select_one_indexed_field() {
    let response = create_type_as_tenant_account(harvest_type_payload()).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.json::<Value>().await["indexed_field_key"], "ngay");
}

#[tokio::test]
async fn tenant_user_cannot_manage_business_types_or_user_business_permissions() {
    assert_eq!(create_type_as_tenant_user(harvest_type_payload()).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(set_business_permissions_as_tenant_user("viewer", all_business_permissions()).await.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tenant_account_can_disable_only_business_update_for_a_user() {
    let response = set_business_permissions_as_tenant_account("viewer", BusinessPermissionsRequest { read: true, create: true, update: false, delete: true }).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(get_business_permissions_as_tenant_account("viewer").await.json::<Value>().await["update"], false);
}
```

- [ ] **Step 2: Run the management tests and verify they fail**

Run: `cargo test -p iot-nano-monolith --test management_business -- --nocapture`

Expected: fail because management routes do not exist.

- [ ] **Step 3: Implement management JSON endpoints**

Mount the six management endpoints in `ManagementSessionRouter`. Every handler
must call `require_tenant_account`; mutations must also use
`authorize_tenant_mutation`. Map validation to 400, unknown type/user to 404,
and index/schema conflicts to 409.

- [ ] **Step 4: Implement the console page**

Add a `Business Data` navigation item. The page provides a type form with
field key/label/type/required controls, Asset Profile multi-select, and one
indexed-field select limited to eligible fields. Render the four user
permission toggles in the existing user management area. Destructive actions
show the exact hard-delete consequence before submission.

- [ ] **Step 5: Run management and template verification**

Run:

```bash
cargo test -p iot-nano-monolith --test management_business -- --nocapture
cargo test -p iot-nano-monolith --test platform_ui_templates -- --nocapture
```

Expected: all tests pass; Tenant User never receives business-management
controls.

- [ ] **Step 6: Commit**

```bash
git add services/iot-nano-monolith/src/management/business.rs \
  services/iot-nano-monolith/src/management/mod.rs \
  services/iot-nano-monolith/src/management/operator_api.rs \
  services/iot-nano-monolith/src/management/routes/tenant.rs \
  services/iot-nano-monolith/templates/platform_ui/tenant_navigation.html \
  services/iot-nano-monolith/templates/platform_ui/tenant_business.html \
  services/iot-nano-monolith/templates/platform_ui/tenant_users.html \
  services/iot-nano-monolith/tests/management_business.rs \
  services/iot-nano-monolith/tests/platform_ui_templates.rs
git commit -m "feat(console): manage platform business data"
```

### Task 4: Expose OAuth/PAT-protected public business record APIs

**Files:**
- Create: `services/iot-nano-api/src/business.rs`
- Modify: `services/iot-nano-api/src/lib.rs`
- Modify: `services/iot-nano-api/src/public_v1.rs`
- Test: `services/iot-nano-api/tests/business.rs`
- Test: `services/iot-nano-monolith/tests/external_app_contract.rs`

**Consumes:** business storage APIs and the existing public bearer-token
resolver.

**Produces:** `/api/v1/business/*` APIs that authorize with OAuth user tokens
or Tenant Account PATs.

- [ ] **Step 1: Write failing public API tests**

```rust
#[tokio::test]
async fn oauth_user_with_asset_access_and_default_business_permissions_can_crud_records() {
    let fixture = authorized_business_fixture().await;
    let created = fixture.create_record(harvest_record_payload()).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(fixture.query_records(date_range_query()).await.status(), StatusCode::OK);
    assert_eq!(fixture.update_record(created.id(), changed_harvest_record_payload()).await.status(), StatusCode::OK);
    assert_eq!(fixture.delete_record(created.id()).await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn user_without_asset_access_cannot_read_or_write_the_assets_business_records() {
    let fixture = unauthorized_asset_business_fixture().await;
    assert_eq!(fixture.query_records(date_range_query()).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.create_record(harvest_record_payload()).await.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn disabled_business_update_permission_denies_patch_but_keeps_read_access() {
    let fixture = update_disabled_business_fixture().await;
    assert_eq!(fixture.update_record(fixture.record_id, changed_harvest_record_payload()).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.query_records(date_range_query()).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn indexed_date_range_query_returns_only_matching_records() {
    let fixture = authorized_business_fixture().await;
    fixture.create_record(harvest_for("2026-10-07")).await;
    fixture.create_record(harvest_for("2026-11-01")).await;
    let body = fixture.query_records(date_range("2026-10-01", "2026-10-31")).await.json::<Value>().await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run the API tests and verify they fail**

Run: `cargo test -p iot-nano-api --test business -- --nocapture`

Expected: fail because `/api/v1/business/*` is not mounted.

- [ ] **Step 3: Implement a focused business router**

Create `business.rs` and merge its router into `public_v1::router`. Reuse the
existing bearer-token extraction and principal resolution. For an OAuth user,
load `business_user_permissions` and verify the Asset through existing public
Asset authorization. For a Tenant Account PAT principal, grant the same
effective business rights as the Tenant Account and require the record tenant
to match the PAT tenant.

Use these handlers:

```rust
async fn list_business_types_for_asset(State(context): State<PublicApiContext>, headers: HeaderMap, Query(query): Query<BusinessTypeAssetQuery>) -> Result<Json<Vec<BusinessTypeResponse>>, PublicApiError>;
async fn query_business_records(State(context): State<PublicApiContext>, headers: HeaderMap, Json(request): Json<BusinessRecordQueryRequest>) -> Result<Json<BusinessRecordPage>, PublicApiError>;
async fn create_business_record(State(context): State<PublicApiContext>, headers: HeaderMap, Json(request): Json<CreateBusinessRecordRequest>) -> Result<(StatusCode, Json<BusinessRecordResponse>), PublicApiError>;
async fn update_business_record(State(context): State<PublicApiContext>, headers: HeaderMap, Path(record_id): Path<Uuid>, Json(request): Json<UpdateBusinessRecordRequest>) -> Result<Json<BusinessRecordResponse>, PublicApiError>;
async fn delete_business_record(State(context): State<PublicApiContext>, headers: HeaderMap, Path(record_id): Path<Uuid>) -> Result<StatusCode, PublicApiError>;
```

- [ ] **Step 4: Keep the external contract constrained**

Update `external_app_contract.rs` so Power Monitor uses only `/api/v1/business/*`
through its BFF and OAuth/PAT bearer resolution remains centralized.

- [ ] **Step 5: Run public API verification**

Run:

```bash
cargo test -p iot-nano-api --test business -- --nocapture
cargo test -p iot-nano-api --test public_v1 -- --nocapture
cargo test -p iot-nano-monolith --test external_app_contract -- --nocapture
```

Expected: CRUD, Asset access intersection, permissions, PAT, and indexed range
queries pass.

- [ ] **Step 6: Commit**

```bash
git add services/iot-nano-api/src/business.rs services/iot-nano-api/src/lib.rs \
  services/iot-nano-api/src/public_v1.rs services/iot-nano-api/tests/business.rs \
  services/iot-nano-monolith/tests/external_app_contract.rs
git commit -m "feat(api): expose authorized business records"
```

### Task 5: Add Power Monitor virtual-table and record editor UX

**Files:**
- Modify: `apps/powermonitor/lib/browser-api.ts`
- Create: `apps/powermonitor/components/business-records-panel.tsx`
- Modify: `apps/powermonitor/components/powermonitor-dashboard.tsx`
- Modify: `apps/powermonitor/app/globals.css`
- Test: `apps/powermonitor/tests/browser-api.test.ts`
- Test: `apps/powermonitor/tests/business-records-panel.test.tsx`
- Test: `apps/powermonitor/tests/dashboard-contract.test.tsx`

**Consumes:** public business API from Task 4 via the existing `/api/v1/[...path]`
BFF proxy.

**Produces:** a user-facing virtual table per business type for the selected
Asset, including a type-aware indexed filter and dynamic CRUD forms.

- [ ] **Step 1: Write failing browser API tests**

```ts
it("queries records using the business type indexed range", async () => {
  await queryBusinessRecords({ assetId: "farm-a", businessTypeId: "harvest", indexedFrom: "2026-10-01", indexedTo: "2026-10-31" });
  expect(fetch).toHaveBeenCalledWith("/api/v1/business/records/query", expect.objectContaining({ method: "POST" }));
});
```

- [ ] **Step 2: Write failing panel tests**

```tsx
it("renders only business types allowed for the selected asset profile", async () => {
  render(<BusinessRecordsPanel assetId="farm-a" />);
  expect(await screen.findByRole("button", { name: "Sản lượng" })).toBeTruthy();
});

it("uses the type indexed date field for the range filter", async () => {
  fireEvent.change(screen.getByLabelText("From"), { target: { value: "2026-10-01" } });
  fireEvent.change(screen.getByLabelText("To"), { target: { value: "2026-10-31" } });
  expect(queryBusinessRecords).toHaveBeenCalledWith(expect.objectContaining({ indexedFrom: "2026-10-01" }));
});

it("hides create update delete controls when the Platform denies the action", async () => {
  mockBusinessApi({ permissions: { read: true, create: false, update: false, delete: false } });
  render(<BusinessRecordsPanel assetId="farm-a" />);
  expect(screen.queryByRole("button", { name: "Create record" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Edit record" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Delete record" })).toBeNull();
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cd apps/powermonitor
npm test -- --run tests/browser-api.test.ts tests/business-records-panel.test.tsx
```

Expected: fail because business client functions and panel do not exist.

- [ ] **Step 4: Implement BFF client types and operations**

Add `BusinessType`, `BusinessField`, `BusinessRecord`, and
`BusinessRecordQuery` to `browser-api.ts`. Add `listBusinessTypes`,
`queryBusinessRecords`, `createBusinessRecord`, `updateBusinessRecord`, and
`deleteBusinessRecord`; all must use the existing `request`/`jsonRequest`
helpers so OAuth access tokens remain BFF-only.

- [ ] **Step 5: Implement the panel and integrate it with Asset selection**

Render the panel only for a persisted selected Asset. Let the user select one
logical business type, show its dynamic fields, and render list rows from
`data`. Date/number/datetime indexed fields render a range control;
string/boolean/select indexed fields render one equality control. Create/edit
validates required fields client-side for feedback, but treats Platform
validation as authoritative. Delete requires an explicit confirmation because
it is a hard delete.

- [ ] **Step 6: Run Power Monitor verification**

Run:

```bash
cd apps/powermonitor
npm test
npm run build
```

Expected: all tests pass and Next lists no client-side access to secrets or
Platform DB configuration.

- [ ] **Step 7: Commit**

```bash
git add apps/powermonitor/lib/browser-api.ts \
  apps/powermonitor/components/business-records-panel.tsx \
  apps/powermonitor/components/powermonitor-dashboard.tsx \
  apps/powermonitor/app/globals.css \
  apps/powermonitor/tests/browser-api.test.ts \
  apps/powermonitor/tests/business-records-panel.test.tsx \
  apps/powermonitor/tests/dashboard-contract.test.tsx
git commit -m "feat(powermonitor): manage asset business records"
```

### Task 6: Run cross-layer verification and update operational docs

**Files:**
- Modify: `docs/superpowers/specs/2026-10-07-platform-business-data-design.md`
- Modify: `install/docker/README.md`
- Modify: `scripts/install-linux-monolith.sh`
- Test: `debug/test_seed_powermonitor.py`

**Consumes:** complete Platform and Power Monitor business-data implementation.

**Produces:** reproducible demo, documentation, and cross-layer verification.

- [ ] **Step 1: Write a failing demo assertion**

```python
def test_demo_business_type_and_indexed_harvest_records_are_available(self):
    business_types = self.management_get("/api/v1/management/business/types")
    self.assertEqual(business_types[0]["indexed_field_key"], "ngay")
```

- [ ] **Step 2: Add only configuration demo data**

Extend the existing seed script with one `Sản lượng` business type and allowed
Asset Profile ids. Do not seed telemetry or business record rows.

- [ ] **Step 3: Document hard-reset behavior**

Document that schema version 9 requires a fresh development database and that
`start-demo.sh --clean` drops the demo Platform volume before reseeding.

- [ ] **Step 4: Run full verification**

Run:

```bash
cargo test -p iot-storage --test business -- --nocapture
cargo test -p iot-nano-api --test business -- --nocapture
cargo test -p iot-nano-monolith --test management_business -- --nocapture
cd apps/powermonitor && npm test && npm run build
bash scripts/test-install-linux-monolith.sh
bash scripts/test-docker-install.sh
```

Expected: all commands exit 0. Manually run `bash install/docker/start-demo.sh
--clean`, log in as `user1`, open a Farm Asset, select `Sản lượng`, create a
dated record, and verify the indexed date-range filter returns it.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-10-07-platform-business-data-design.md \
  install/docker/README.md scripts/install-linux-monolith.sh \
  debug/test_seed_powermonitor.py
git commit -m "docs: document business data deployment"
```

## Self-review

- **Spec coverage:** Tasks 1–2 implement the fixed Platform DB model, field
  rules, versioning, hard deletes, and expression index. Task 3 covers Tenant
  Account management and global default user permissions. Task 4 covers OAuth,
  PAT, Asset access, and public record CRUD. Task 5 covers the Power Monitor
  virtual table and dynamic form. Task 6 covers seeds and end-to-end checks.
- **No dynamic record table:** all tasks use only `business_records`; dynamic
  DDL is limited to one validated expression index per business type.
- **Type consistency:** the `BusinessTypeDefinition`, `BusinessRecord`, and
  `BusinessUserPermissions` interfaces introduced in Task 1 are used by each
  later task with the same identifiers and action names.
