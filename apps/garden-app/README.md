# Garden App

Garden App is a compact farm-operations UI that uses a single local `admin`
account and the IoT Nano User PAT of the selected tenant user.

It keeps only farm, area, crop-zone, device-to-zone mapping, and production
records in its local SQLite database. Devices, telemetry, commands, platform
alert rules, and alert incidents remain in IoT Nano.

## Run locally

Start IoT Nano first:

```bash
./scripts/dev/cargo-lane.sh local-platform -- build -p iot-nano-monolith
```

Run the local monolith with the local runtime helper, then start Garden App:

```bash
cd apps/garden-app
npm install
npm run dev -- --port 3003
```

Open `http://localhost:3003/login` and sign in with `admin` / `admin`.

Garden App reads `IOT_NANO_BASE_URL` and `IOT_NANO_USER_PAT` from `.env.local`.
On first load, it stores the PAT in its local `data/garden-app.sqlite` so it
survives an app restart. Never commit `.env.local` or `data/`.

## User PAT

In IoT Nano, sign in as the Tenant Account and open:

```text
Tenant Account → Personal access tokens
```

Select the user who owns or manages the devices, then create or rotate that
user’s PAT. The token has fixed platform scopes for assets, devices,
telemetry, alerts, commands, rules, and business data. Resource permission
still applies: a user must manage a device to create its alert rules.

For an API-first IoT Nano Platform integration guide for building an external
Garden-style app, see [API_GUIDE.md](API_GUIDE.md).

## Main workflows

1. Create Farm → Area → Crop zone.
2. Claim a device with Serial Number and Pairing Code in **Thiết bị riêng**.
3. Assign the device to a crop zone.
4. In **Phân vùng**, press a device card to view recent telemetry and send
   `setRelay` ON/OFF commands.
5. In **Cảnh báo**, select a crop zone and create a rule. Garden App creates
   one IoT Nano alert rule for every device assigned to that zone. Rules are
   evaluated by IoT Nano, not by Garden SQLite.
6. Acknowledge an incident in Garden App. Acknowledgement removes it from the
   active notification list; IoT Nano retains it for audit until its condition
   resolves.

## Telemetry and charts

The device-card dialog polls the most recent 15-minute telemetry window every
5 seconds while open. The device page chart uses the latest available
telemetry and displays time on the X axis and metric value on the Y axis.

## Verification

```bash
cd apps/garden-app
npm test
npm run build
```

## Simulator

The Power Meter simulator is at
[`power-device-simulator`](power-device-simulator/README.md). It publishes
QoS 1 telemetry and handles two-way `setRelay` RPC commands.
