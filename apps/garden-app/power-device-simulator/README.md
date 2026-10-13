# IoT Nano Power Meter device simulator

The simulator authenticates to local IoT Nano as one device, connects to MQTT
TCP at `127.0.0.1:18883`, publishes QoS 1 Power Meter telemetry, and supports
IoT Nano two-way RPC.

```bash
cd apps/garden-app/power-device-simulator
python3 -m pip install -r requirements.txt
export IOT_NANO_DEVICE_TOKEN='your-32-character-device-token'
python3 power_device.py
```

Optional environment variables:

- `IOT_NANO_MQTT_HOST` (default `127.0.0.1`)
- `IOT_NANO_MQTT_PORT` (default `18883`)
- `POWER_TELEMETRY_SECONDS` (default `10`)

Telemetry uses `v1/devices/me/telemetry` and contains `power_w`, `current_a`,
and cumulative `energy_kwh` measurements.

The simulator subscribes QoS 1 to `v1/devices/me/rpc/request/+` and returns a
QoS 1, non-retained response on the matching
`v1/devices/me/rpc/response/{id}` topic. It supports these two-way methods:

- `sample_now`: publishes and returns a new telemetry sample.
- `reboot`: creates a new boot ID and resets the telemetry sequence, while
  preserving cumulative energy.
- `switch_on` / `switch_off`: acknowledge the app control command and print the
  resulting control state in the simulator log.
- `setRelay`: the Garden App control contract; accepts
  `{ "enabled": true | false }`, returns `relay_on`, and prints the resulting
  ON/OFF state.
