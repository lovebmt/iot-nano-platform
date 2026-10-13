# Asset Business Data Editor Design

## Goal

Let tenant administrators and authorized tenant users manage Business Records
in Platform Management while editing an asset. The editor appears as a
Business Data section at the bottom of the existing asset edit screen.

## Scope

- Add a Business Data section to Platform Management → Assets → Edit Asset.
- List Business Types that apply to the edited asset's profile.
- Let the operator select a Business Type and view that asset's records of the
  selected type.
- Add field-based record filtering to the management editor and the public
  Business Record query API.
- Provide schema-driven add and edit forms, plus confirmed delete actions.
- Enforce existing Business Data read, create, update, and delete permissions.

The PowerMonitor UI is not changed. Existing public Business API clients keep
using Bearer Personal Access Tokens; the new browser editor uses the existing
management-session cookie and does not expose a token to the browser.

## User Experience

The asset editor retains its current fields and actions. Its final section is
Business Data:

1. A type selector contains only Business Types valid for the asset profile.
2. Choosing a type shows that type's existing records in a table.
3. Add record opens an inline, schema-driven form. Edit uses the same form
   populated with the record's values.
4. Delete requires confirmation before the record is removed.
5. A filter row is generated from the selected type's schema. Text, boolean,
   and enum fields use equality filters. Date and numeric fields use inclusive
   `from` and `to` range filters.
6. Multiple non-empty filters are combined with AND.
7. If no type applies, the section explains that no Business Types are
   configured for this asset profile.

Fields are rendered from the Business Type schema and retain their native
input semantics: dates as date inputs, decimal and integer values as numeric
inputs, booleans as checkboxes, enum values as selects, and other values as
text inputs.

### Business Type Builder Layout

The Business Data configuration page uses a two-column builder on desktop:

- The left rail holds the Business Type name, Asset Profile scope, indexed
  field selection, destructive-operation guidance, and the primary submit
  action.
- The right workspace holds the schema field rows. Each compact row aligns
  field key, label, type, select options, required state, and remove action.
- The Add field control follows the field rows, keeping schema editing in one
  visual group.
- At narrow widths, the builder stacks into a single column without changing
  field order or hiding controls.

The Business Type list remains below the builder and is not redesigned as part
of this layout change.

## Authorization

Tenant accounts have full access. Tenant users require both access to the
asset and the matching Business Data permission for each operation:

- `read` to view the section and record table;
- `create` to add records;
- `update` to edit records;
- `delete` to delete records.

The UI hides unavailable actions for usability, but server-side management
routes independently enforce the same checks. Unauthorized or cross-tenant
records are never revealed.

## Server Design

Add management-session routes for Business Record query, create, update, and
delete operations scoped to an asset. The handlers obtain the current session
principal, resolve the asset and applicable Business Type through the existing
repositories, and delegate record persistence and schema validation to the
Business Repository.

Extend the existing public `POST /api/v1/business/records/query` contract,
which currently filters only the configured indexed field. The replacement
contract accepts a list of field filters:

```json
{
  "asset_id": "uuid",
  "business_type_id": "uuid",
  "filters": [
    { "field_key": "status", "equals": "open" },
    { "field_key": "measured_on", "from": "2026-10-01", "to": "2026-10-31" }
  ],
  "limit": 50,
  "cursor": "uuid"
}
```

Each filter key must exist in the selected Business Type and its value must
match that field's declared type. Equality is permitted for text, boolean,
and enum fields. Date and numeric fields use one or both inclusive range
bounds. Filters are ANDed. The query remains tenant-, asset-, and
Business-Type-scoped; pagination remains cursor based. Storage may use the
configured indexed field as an optimization, but query correctness must not
depend on the field being indexed.

The public Business API continues to accept Bearer Personal Access Tokens for
programmatic record CRUD and query. Management-session routes exist solely for
the server-rendered Platform Management editor.

## Errors and Refreshes

- Field validation errors are shown next to the relevant form field.
- A missing or incompatible Business Type yields an asset-editor error state.
- Permission failures return forbidden responses and leave the table unchanged.
- Delete confirmation prevents an accidental destructive action.
- Successful mutations refresh the selected type's table and preserve the
  selected type.
- An invalid field key, an incompatible filter value, or an unsupported filter
  operator returns a validation error without executing a query.

## Verification

- Management router tests cover tenant-account CRUD, user permission allow and
  deny cases, asset/type compatibility, field-filter validation, AND semantics,
  pagination, and cross-tenant isolation.
- Template tests verify the Business Data section, type selector, dynamic form
  markers, and permission-conditioned controls.
- Browser-facing behavior is verified against the management-session routes;
  existing public API tests continue to cover Personal Access Token CRUD and
  the expanded field-filter query contract.
