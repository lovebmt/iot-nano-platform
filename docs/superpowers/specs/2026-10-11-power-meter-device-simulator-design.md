# Power Meter device simulator

## Goal

Convert the former garden-pump MQTT simulator into a Power Meter simulator
that works with the local Power Monitor application and IoT Nano.

## MQTT connection

The simulator connects to MQTT TCP at `127.0.0.1:18883` by default. Existing
environment overrides for MQTT host, port, device token, and telemetry cadence
remain available, with the telemetry cadence renamed for the meter domain.

## Telemetry

Publish QoS 1 JSON telemetry on `v1/devices/me/telemetry`. Each sample retains
the boot ID, sequence, and UTC event timestamp envelope. Its measurements are:

- `power_w`: instantaneous active power in watts.
- `voltage_v`: line voltage in volts.
- `current_a`: current in amperes.
- `energy_kwh`: a monotonically increasing cumulative energy value.

The generated values are coherent: active power derives from voltage and
current, while energy advances by power times the configured telemetry interval.

## Two-way RPC

Subscribe QoS 1 to `v1/devices/me/rpc/request/+`. For each valid pending
two-way command, publish a QoS 1 non-retained JSON response to
`v1/devices/me/rpc/response/{command_id}`.

- `sample_now` creates a sample immediately, publishes it, and returns it in a
  successful response.
- `reboot` creates a new boot ID, resets the sequence counter, and returns the
  resulting device state.
- Unknown or malformed commands return `{ "ok": false, "error": ... }` on
  the matching response topic without changing meter state.

## Verification

Unit tests will cover electrical telemetry shape/coherence, cumulative energy,
and valid and invalid two-way RPC responses. A local run will then confirm the
simulator reaches the configured broker or reports a useful connection error.
