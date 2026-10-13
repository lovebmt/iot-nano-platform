# User Personal Access Token Design

## Goal

Replace tenant-account PATs with user PATs. A tenant account manages one active
PAT for each tenant user; each PAT is subject to both fixed default scopes and
the user's existing ownership, share, and capability permissions.

## Storage

- Replace `tenant_personal_access_tokens` with a user-bound token table.
- Store `tenant_id`, `user_id`, name, hash, prefix, created timestamp, and
  revoked timestamp. Never store a token secret.
- Enforce one active token per user. Creating a token revokes that user's prior
  active token atomically.
- Development SQLite/Timescale state is reset for this schema replacement.

## Fixed Scopes

Every User PAT receives exactly these scopes. The tenant UI does not expose
scope checkboxes:

```text
assets:read, assets:write
devices:read, devices:write
telemetry:read
commands:read, commands:write
rules:read, rules:write
business:read, business:write
```

`business:write` permits create, update, and delete of Business Records only.
Business Type management remains tenant-account-only.

## Authorization

- A PAT resolves to its user principal and tenant.
- Scope validation is required first.
- Resource operations then enforce the user's actual owner/share permission and
  required user capability. A PAT cannot grant more access than its user.
- Device claim requires `devices:write` and `claim_devices`; a successful claim
  makes the device owned by that user.

## Tenant UI

The Personal Access Tokens page lists tenant users and token metadata without
secrets. The tenant account selects a user, creates/rotates that user's PAT,
copies the returned one-time secret, or revokes the active token.
