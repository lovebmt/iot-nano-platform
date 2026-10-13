# Platform business data design

## Scope

IoT Nano Platform gains a general business-data module. It stores manual,
asset-bound data such as daily harvest output and notes. Power Monitor remains
stateless: it calls the Platform business API through its existing BFF.

The module is stored in the existing Platform database:

- SQLite: new tables in `platform.sqlite`.
- PostgreSQL/Timescale: new tables in the existing Platform schema.

No table is created dynamically per business type.

## Data model

### `business_types`

One row per Tenant-created business type. It contains the type id, tenant id,
display name, active schema version, and lifecycle metadata.

### `business_type_versions`

One row for each schema revision. It contains:

- `fields_json`: ordered custom fields with key, label, type, and required flag.
- `allowed_asset_profile_ids_json`: Asset Profiles on which the type can be
  used.

Supported field types are `string`, `integer`, `decimal`, `boolean`, `date`,
`datetime`, and `select`.

### `business_records`

Each record has `id`, `tenant_id`, `asset_id`, `business_type_id`,
`schema_version`, `data_json`, creator/updater identity, and timestamps.

Records are queried and authorized by tenant, type, and Asset. `data_json` is
validated against the current business type version before every create or
update.

`business_records` is the only table that stores business-record values. Each
business type may select one field from its validated schema for one expression
index on that fixed table. Numeric and date-like fields support indexed range
queries; string, boolean, and select fields support indexed equality queries.
The key and expression are derived from the saved schema, never from raw query
input.

The business type, version, and permission tables remain fixed Platform tables.
No `business_record_values` alternative is used, and no record table is
created per business type, tenant, or Asset Profile.

### `business_user_permissions`

One row per tenant user. It holds the four global business permissions:
`read`, `create`, `update`, and `delete`. All four are enabled by default when
a Tenant User is created. Tenant Account can enable or disable each flag.

## Authorization

Business authorization is the intersection of:

1. the user has the required global business permission; and
2. the user already has access to the target Asset through Platform resource
   authorization.

Permissions are not configured per business type or Asset Profile in v1.
Tenant Account administers business types, versions, Asset Profile assignment,
and user business permissions.

## Schema and deletion rules

- Field type is immutable after the field is created.
- Adding a field creates a new type version; old records have no value for it.
- Removing a field creates a new type version and hard-removes that key from
  every record of the type.
- Removing an Asset Profile from a business type hard-deletes all records of
  that type belonging to Assets with that profile.
- Deleting a record is a hard delete.
- Deleting a business type hard-deletes its versions, records, and related
  configuration.

All destructive schema changes execute transactionally in the Platform DB.

## API and UI boundary

The Platform exposes the module under `/api/v1/business/*`.

- Tenant Console manages business types, their fields and Asset Profile scope,
  and the four user permissions.
- Power Monitor lists, creates, updates, and deletes records for an Asset only
  when the authenticated user passes the Platform authorization checks.

The existing OAuth user token and Tenant Account PAT authentication flow is
unchanged.

## Verification

Tests cover tenant isolation, Asset authorization, default permissions,
schema validation, immutable field types, and each specified cascading delete
rule for both SQLite and PostgreSQL/Timescale storage.
