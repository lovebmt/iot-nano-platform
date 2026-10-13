#!/usr/bin/env python3
"""IoT Nano Power Meter simulator: telemetry plus two-way RPC."""

from __future__ import annotations

import json
import os
import random
import sys
import threading
import time
import uuid
from datetime import datetime, timezone

import paho.mqtt.client as mqtt


TELEMETRY_TOPIC = "v1/devices/me/telemetry"
RPC_REQUEST_FILTER = "v1/devices/me/rpc/request/+"
RPC_RESPONSE_PREFIX = "v1/devices/me/rpc/response/"


class PowerMeterSimulator:
    def __init__(self) -> None:
        self.token = os.environ.get("IOT_NANO_DEVICE_TOKEN", "LT9inBMDgQp9ssP2a6hCqxKdCsew91Z1").strip()
        if not self.token:
            raise RuntimeError("Set IOT_NANO_DEVICE_TOKEN before starting the simulator.")
        self.host = os.environ.get("IOT_NANO_MQTT_HOST", "127.0.0.1")
        self.port = int(os.environ.get("IOT_NANO_MQTT_PORT", "18883"))
        self.interval = float(os.environ.get("POWER_TELEMETRY_SECONDS", "10"))
        self.boot_id = str(uuid.uuid4())
        self.sequence = 0
        self.energy_kwh = 0.0
        self.relay_on = False
        self.connected = threading.Event()
        self.connection_failed = threading.Event()
        self.client = mqtt.Client(
            mqtt.CallbackAPIVersion.VERSION2,
            client_id=f"power-meter-{uuid.uuid4().hex[:12]}",
            protocol=mqtt.MQTTv311,
        )
        self.client.username_pw_set("iotd_device_token", password=self.token)
        self.client.on_connect = self.on_connect
        self.client.on_message = self.on_message
        self.client.on_disconnect = self.on_disconnect

    def on_connect(self, client: mqtt.Client, _userdata: object, _flags: object, reason_code: object, _properties: object) -> None:
        if reason_code != 0:
            print(f"MQTT authentication failed: {reason_code}", file=sys.stderr)
            self.connection_failed.set()
            client.disconnect()
            return
        result, _mid = client.subscribe(RPC_REQUEST_FILTER, qos=1)
        if result != mqtt.MQTT_ERR_SUCCESS:
            print(f"Cannot subscribe to RPC requests: {result}", file=sys.stderr)
            self.connection_failed.set()
            client.disconnect()
            return
        self.connected.set()
        print(f"Connected to mqtt://{self.host}:{self.port}; waiting for Power Meter RPC commands.")

    def on_disconnect(self, _client: mqtt.Client, _userdata: object, _disconnect_flags: object, reason_code: object, _properties: object) -> None:
        if reason_code != 0:
            print(f"Disconnected from MQTT ({reason_code}); paho will retry.", file=sys.stderr)

    def on_message(self, client: mqtt.Client, _userdata: object, message: mqtt.MQTTMessage) -> None:
        command_id = message.topic.removeprefix("v1/devices/me/rpc/request/")
        if not command_id or command_id == message.topic:
            return
        try:
            request = json.loads(message.payload.decode("utf-8"))
            if request.get("id") != command_id:
                raise ValueError("request id does not match topic")
            method = request.get("method")
            if method == "sample_now":
                response = {"ok": True, "result": self.publish_telemetry()}
            elif method == "reboot":
                self.boot_id = str(uuid.uuid4())
                self.sequence = 0
                response = {
                    "ok": True,
                    "result": {
                        "boot_id": self.boot_id,
                        "sequence": self.sequence,
                        "energy_kwh": self.energy_kwh,
                    },
                }
            elif method == "setRelay":
                enabled = request.get("params", {}).get("enabled")
                if not isinstance(enabled, bool):
                    raise ValueError("params.enabled must be boolean")
                self.relay_on = enabled
                response = {"ok": True, "result": {"relay_on": self.relay_on}}
                print(f"Power Meter relay {'ON' if self.relay_on else 'OFF'} by app RPC {command_id}")
            elif method == "switch_on":
                response = {"ok": True, "result": {"state": "on"}}
                print(f"Power Meter switched ON by app RPC {command_id}")
            elif method == "switch_off":
                response = {"ok": True, "result": {"state": "off"}}
                print(f"Power Meter switched OFF by app RPC {command_id}")
            else:
                raise ValueError("unsupported method")
            if method in {"sample_now", "reboot"}:
                print(f"Power Meter handled {method} RPC {command_id}")
        except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
            response = {"ok": False, "error": str(error)}
            print(f"Rejected Power Meter RPC {command_id}: {error}", file=sys.stderr)
        client.publish(
            f"{RPC_RESPONSE_PREFIX}{command_id}",
            json.dumps(response, separators=(",", ":")),
            qos=1,
            retain=False,
        )

    def telemetry(self) -> dict[str, object]:
        self.sequence += 1
        voltage = round(random.uniform(218.0, 242.0), 1)
        current = round(random.uniform(0.2, 4.0), 3)
        power = round(voltage * current, 2)
        self.energy_kwh = round(self.energy_kwh + power * self.interval / 3_600_000, 6)
        return {
            "boot_id": self.boot_id,
            "sequence": self.sequence,
            "event_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "measurements": {
                "power_w": power,
                "current_a": current,
                "energy_kwh": self.energy_kwh,
            },
        }

    def publish_telemetry(self) -> dict[str, object]:
        payload = self.telemetry()
        result = self.client.publish(
            TELEMETRY_TOPIC,
            json.dumps(payload, separators=(",", ":")),
            qos=1,
            retain=False,
        )
        if result.rc != mqtt.MQTT_ERR_SUCCESS:
            print(f"Telemetry publish failed: {result.rc}", file=sys.stderr)
        return payload

    def run(self) -> None:
        self.client.connect(self.host, self.port, keepalive=45)
        self.client.loop_start()
        try:
            if not self.connected.wait(timeout=10):
                if self.connection_failed.is_set():
                    raise RuntimeError("MQTT broker rejected this device token.")
                raise RuntimeError("Timed out waiting for MQTT connection.")
            while True:
                payload = self.publish_telemetry()
                print(f"Power Meter telemetry #{payload['sequence']}: power_w={payload['measurements']['power_w']}")
                time.sleep(self.interval)
        except KeyboardInterrupt:
            print("Stopping Power Meter simulator.")
        finally:
            self.client.loop_stop()
            self.client.disconnect()


if __name__ == "__main__":
    PowerMeterSimulator().run()
