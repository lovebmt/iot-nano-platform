# Scenario guide: build an external app on IoT Nano Platform

This document simulates how a new Garden-style application is built on IoT
Nano Platform. The application itself does **not** exist at the start of this
guide. First configure the tenant in IoT Nano Console, then use the User PAT
from command line/API, then implement your own app.

```text
System Console → creates tenant
Tenant Console → creates user, device, serial, pairing code, User PAT
External app/API client → claims device and calls IoT Nano APIs
Device → MQTT telemetry and RPC response
```

## Step 0 — Start and open IoT Nano

For local development, start IoT Nano and open the console:

```text
http://localhost:18081/login
```

There are three console roles:

| Role | Responsibility |
| --- | --- |
| System Account | Create/suspend tenants and inspect system metrics. |
| Tenant Account | Create tenant users, provision devices, generate pairing codes, create User PATs. |
| Tenant user | Uses a User PAT from the external app; owns/manages selected resources. |

## Step 1 — Create the tenant in System Console

1. Sign in as **System Account**.
2. Go to **System Console → Create tenant**.
3. Enter tenant slug, Tenant Account username, and Tenant Account password.
4. Sign out, then sign in using the newly created **Tenant Account**.

At this point the tenant exists but has no normal app user and no device.

## Step 2 — Create the app user in Tenant Console

1. Open **Tenant Account → Users**.
2. Create a normal user, for example `garden-operator`.
3. Give the user required capabilities. For a Garden app this commonly means:
   `claim_devices`, `control_devices`, and any capability needed to create or
   edit resources.

Resource permission is separate from API scope. A User PAT may have
`rules:write`, but the user still needs ownership or Manager permission for the
device whose rule it creates.

## Step 3 — Provision the device in Tenant Console

1. Open **Tenant Account → Devices**.
2. Choose **Provision device**.
3. Set:
   - **Serial Number**: a stable human/device serial, for example `PUMP-001`.
   - **Display name** and **Device Profile**.
4. Save the device.

The Serial Number is the value your external app will later send to the claim
API. It is not the internal `device_id` UUID/string returned by the platform.

## Step 4 — Generate a pairing code

1. In **Tenant Account → Devices**, open the device editor.
2. In **Pairing code**, choose **Generate new code**.
3. Copy the code immediately. A new code revokes the prior code and the code
   has an expiry.

Keep these values for the next step:

```text
Serial Number: PUMP-001
Pairing Code:  <one-time code from console>
```

## Step 5 — Create a User PAT

1. Open **Tenant Account → Personal access tokens**.
2. Select `garden-operator`.
3. Create a token, copy it once, and store it in your external app server
   secret store.
4. The Personal Access Token page shows the fixed permissions attached to the
   PAT: assets, devices, telemetry, alerts, commands, rules, and business
   data read/write scopes.

Set the shell variables for direct API testing:

```bash
export IOT_NANO_URL=http://127.0.0.1:18081
export IOT_NANO_USER_PAT='paste-the-user-pat-here'
```

Never put this PAT in browser JavaScript, a mobile binary, Git, or a log.

## Step 6 — Test the same APIs an external app will call

### 6.1 List devices visible to the user

```bash
curl -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  "$IOT_NANO_URL/api/v1/devices"
```

Before a device is claimed, it may not appear in this user’s resource list.

### 6.2 Claim the device using serial number and pairing code

```bash
curl -X POST \
  -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  -H 'content-type: application/json' \
  -d '{"serial_number":"PUMP-001","code":"PAIRING-CODE-FROM-CONSOLE"}' \
  "$IOT_NANO_URL/api/v1/devices/claim"
```

The response contains `device_id`. Save it in your external app’s database as
the IoT Nano device reference.

### 6.3 Get device information after claim

```bash
curl -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  "$IOT_NANO_URL/api/v1/devices/DEVICE_ID"
```

The external app can now show serial number, profile, ownership/access, and
last-seen information without copying device records into its own database.

## Step 7 — Device sends telemetry via MQTT

The physical device uses its **device token**, not the User PAT. Publish QoS 1
telemetry as the authenticated device:

```bash
mosquitto_pub -h 127.0.0.1 -p 18883 \
  -u iotd_device_token -P "$IOT_NANO_DEVICE_TOKEN" \
  -t 'v1/devices/me/telemetry' -q 1 \
  -m '{"boot_id":"11111111-1111-1111-1111-111111111111","sequence":1,"measurements":{"power_w":725.4,"current_a":3.1,"energy_kwh":0.08}}'
```

The external app reads telemetry using its user PAT:

```bash
curl -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  "$IOT_NANO_URL/api/v1/telemetry?device_id=DEVICE_ID&limit=100"
```

## Step 8 — One-way and two-way RPC

Create a two-way relay command:

```bash
curl -X POST \
  -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  -H 'content-type: application/json' \
  -H 'idempotency-key: REPLACE-WITH-A-NEW-UUID' \
  -d '{"method":"setRelay","params":{"enabled":true},"mode":"two_way"}' \
  "$IOT_NANO_URL/api/v1/devices/DEVICE_ID/commands"
```

IoT Nano persists the command in its outbox before publishing MQTT. The
command worker receives an immediate in-process wake-up; its periodic poll is
only a recovery/retry fallback.

The device receives:

```text
v1/devices/me/rpc/request/COMMAND_ID
```

For `two_way`, it must reply QoS 1 on:

```text
v1/devices/me/rpc/response/COMMAND_ID
```

Use `"mode":"one_way"` if a result is unnecessary. Poll command lifecycle
with:

```bash
curl -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  "$IOT_NANO_URL/api/v1/commands/COMMAND_ID"
```

## Step 9 — Build app-specific business workflow

This is the point where the external Garden app starts adding its own data and
screens. Choose a single owner for each concept.

| Implement in the external app database | Use IoT Nano Platform |
| --- | --- |
| Farm, area, crop zone, planting layout, local workflow/UI state | Device identity, profiles, ownership, telemetry, MQTT, commands |
| App-local login/session | Alert rules, incidents, audit |
| App-only draft notes | Shared typed Business Data |

For example, an external Garden app may keep **farm / area / crop zone** in
its own SQLite or PostgreSQL database. It maps `crop_zone_id → device_id` and
uses the IoT Nano APIs above for the device behavior.

For shared yield, quality, batch, or compliance records, use IoT Nano Business
Data instead:

| Purpose | IoT Nano endpoint |
| --- | --- |
| List types on an asset | `GET /api/v1/business/types?asset_id=ASSET_ID` |
| Query records | `POST /api/v1/business/records/query` |
| Create record | `POST /api/v1/business/records` |
| Update/delete record | `PUT` / `DELETE /api/v1/business/records/RECORD_ID` |

## Step 10 — Create platform rules from app zones

IoT Nano rules are device-scoped. If an external app creates a rule for one
crop zone, it should create one platform rule for each mapped device and roll
back/archive already-created rules if any later create fails.

```bash
curl -X POST \
  -H "Authorization: Bearer $IOT_NANO_USER_PAT" \
  -H 'content-type: application/json' \
  -d '{"name":"Zone A power high","enabled":true,"metric_key":"power_w","rule_type":"event_threshold","comparison":"gt","threshold":700,"window_seconds":null,"for_seconds":0,"resolve_after_seconds":300,"reopen_grace_seconds":3600,"hysteresis":null,"severity":"warning","reminder_interval_seconds":86400}' \
  "$IOT_NANO_URL/api/v1/devices/DEVICE_ID/alert-rules"
```

Read incidents with `GET /api/v1/alerts` and acknowledge with
`POST /api/v1/alerts/ALERT_ID/acknowledge`.

## Reference code

`apps/garden-app/` is one small reference implementation of this scenario. It
is not required by IoT Nano Platform and should not be treated as a platform
service. Run its checks with:

```bash
cd apps/garden-app
npm test
npm run build
```
