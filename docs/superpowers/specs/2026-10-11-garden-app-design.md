# Garden App Design

## Goal

Create `apps/garden-app`, a simple local farm-operations app that manages
farms, growing areas, crop zones, devices, production records, alerts, and
irrigation controls without using IoT Nano device-to-asset relations.

## Runtime and Security

- Garden App is an independent Next.js application.
- Its SQLite database lives at `data/garden-app.sqlite` under the app root and
  owns all Garden App users, hierarchy, assignments, production records, alert
  rules, and simulator state.
- Garden App has one local account: `admin` / `admin`.
- The tenant PAT is configured only in `apps/garden-app/.env.local`; server
  routes use it to call IoT Nano. Browser code never receives the PAT.
- Garden App does not create or read IoT Nano device-to-asset relations.

## Local Domain Model

```text
Farm
  └─ Area
      └─ Crop zone
          └─ Device assignment (iot_device_id)
```

- A device can be assigned to one crop zone by Garden App.
- A crop zone has production records such as harvest amount and date.
- Alert rules belong to a crop zone and target a device metric such as CO₂,
  temperature, or humidity.

## IoT Nano Integration

- Device pairing accepts a Pairing Code, claims the device through the IoT Nano
  public API using the server-side tenant PAT, then assigns it to a selected
  crop zone in Garden App SQLite.
- Device lists and device telemetry come from IoT Nano APIs through Garden App
  server routes.
- Irrigation controls send the two-way RPC command `irrigation.set` with
  payload `{ "on": true }` or `{ "on": false }`.
- No IoT Nano asset relation is created as a side effect.

## Simulator

Garden App includes a simple simulator route and worker for sample devices:

- irrigation actuator: accepts `irrigation.set` and reports on/off state;
- CO₂ sensor;
- temperature sensor;
- humidity sensor.

The simulator publishes periodic sample telemetry and records command results
through the same Garden App service boundary used by the UI.

## UI

- Local login page and logout action.
- Dashboard with farm tree, alert summary, device cards, and both telemetry and
  production charts.
- Farm, area, and crop-zone create/edit screens.
- Zone page for device assignment by pairing code, business production entry,
  alert rules, telemetry chart, production chart, and irrigation on/off.

## Validation

- Unit tests cover local authentication, SQLite hierarchy constraints, device
  assignment, production aggregation, and alert evaluation.
- Integration tests mock the IoT Nano client boundary to verify PAT remains
  server-side, pairing-code claim behavior, telemetry reads, and irrigation RPC
  requests.
- The simulator is tested for telemetry generation and `irrigation.set`
  command handling.
