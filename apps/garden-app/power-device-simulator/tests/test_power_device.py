import json
import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from power_device import PowerMeterSimulator, TELEMETRY_TOPIC


class FakeClient:
    def __init__(self) -> None:
        self.published: list[tuple[str, str, int, bool]] = []

    def publish(self, topic: str, payload: str, qos: int, retain: bool = False):
        self.published.append((topic, payload, qos, retain))
        return type("PublishResult", (), {"rc": 0})()


class PowerMeterTelemetryTests(unittest.TestCase):
    def setUp(self) -> None:
        patcher = patch.dict(
            os.environ,
            {
                "IOT_NANO_DEVICE_TOKEN": "meter-token",
                "POWER_TELEMETRY_SECONDS": "10",
            },
            clear=False,
        )
        patcher.start()
        self.addCleanup(patcher.stop)
        self.meter = PowerMeterSimulator()
        self.meter.client = FakeClient()

    @patch("power_device.random.uniform", side_effect=[230.0, 2.0, 231.0, 2.0])
    def test_telemetry_contains_power_monitor_measurements_and_cumulative_energy(self, _random) -> None:
        first = self.meter.telemetry()
        second = self.meter.telemetry()

        self.assertEqual(first["sequence"], 1)
        self.assertEqual(second["sequence"], 2)
        self.assertEqual(
            set(first["measurements"]),
            {"power_w", "voltage_v", "current_a", "energy_kwh"},
        )
        self.assertEqual(first["measurements"]["power_w"], 460.0)
        self.assertGreater(second["measurements"]["energy_kwh"], first["measurements"]["energy_kwh"])

    @patch("power_device.random.uniform", side_effect=[230.0, 2.0])
    def test_publish_telemetry_uses_qos_one_without_retaining(self, _random) -> None:
        payload = self.meter.publish_telemetry()

        topic, encoded, qos, retain = self.meter.client.published[0]
        self.assertEqual(topic, TELEMETRY_TOPIC)
        self.assertEqual(qos, 1)
        self.assertFalse(retain)
        self.assertEqual(json.loads(encoded)["measurements"]["power_w"], 460.0)
        self.assertEqual(payload["sequence"], 1)

    def test_configures_iot_nano_device_credentials(self) -> None:
        with patch("power_device.mqtt.Client") as client_factory:
            PowerMeterSimulator()

        client_factory.return_value.username_pw_set.assert_called_once_with(
            "iotd_device_token",
            password="meter-token",
        )


class FakeMessage:
    def __init__(self, command_id: str, request: dict[str, object]) -> None:
        self.topic = f"v1/devices/me/rpc/request/{command_id}"
        self.payload = json.dumps(request).encode("utf-8")


class PowerMeterRpcTests(unittest.TestCase):
    def setUp(self) -> None:
        patcher = patch.dict(os.environ, {"IOT_NANO_DEVICE_TOKEN": "meter-token"}, clear=False)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.meter = PowerMeterSimulator()
        self.client = FakeClient()
        self.meter.client = self.client

    @patch("power_device.random.uniform", side_effect=[230.0, 2.0])
    def test_sample_now_publishes_telemetry_and_matching_two_way_response(self, _random) -> None:
        self.meter.on_message(
            self.client,
            None,
            FakeMessage("command-1", {"id": "command-1", "method": "sample_now", "params": {}}),
        )

        self.assertEqual(self.client.published[0][0], TELEMETRY_TOPIC)
        topic, response, qos, retain = self.client.published[1]
        self.assertEqual(topic, "v1/devices/me/rpc/response/command-1")
        self.assertEqual(qos, 1)
        self.assertFalse(retain)
        self.assertTrue(json.loads(response)["ok"])

    def test_reboot_returns_new_boot_id_and_resets_sequence_without_resetting_energy(self) -> None:
        self.meter.sequence = 8
        self.meter.energy_kwh = 2.5
        old_boot_id = self.meter.boot_id
        self.meter.on_message(
            self.client,
            None,
            FakeMessage("command-2", {"id": "command-2", "method": "reboot", "params": {}}),
        )

        self.assertNotEqual(self.meter.boot_id, old_boot_id)
        self.assertEqual(self.meter.sequence, 0)
        self.assertEqual(self.meter.energy_kwh, 2.5)
        self.assertTrue(json.loads(self.client.published[0][1])["ok"])

    def test_invalid_two_way_request_returns_error_without_changing_meter_state(self) -> None:
        old_boot_id = self.meter.boot_id
        self.meter.on_message(
            self.client,
            None,
            FakeMessage("command-3", {"id": "command-3", "method": "unsupported", "params": {}}),
        )

        self.assertEqual(self.meter.boot_id, old_boot_id)
        response = json.loads(self.client.published[0][1])
        self.assertEqual(self.client.published[0][0], "v1/devices/me/rpc/response/command-3")
        self.assertFalse(response["ok"])
        self.assertEqual(response["error"], "unsupported method")

    def test_switch_on_logs_the_app_control_action(self) -> None:
        with patch("builtins.print") as print_mock:
            self.meter.on_message(
                self.client,
                None,
                FakeMessage("command-on", {"id": "command-on", "method": "switch_on", "params": {}}),
            )

        self.assertTrue(json.loads(self.client.published[0][1])["ok"])
        print_mock.assert_called_once_with("Power Meter switched ON by app RPC command-on")

    def test_switch_off_logs_the_app_control_action(self) -> None:
        with patch("builtins.print") as print_mock:
            self.meter.on_message(
                self.client,
                None,
                FakeMessage("command-off", {"id": "command-off", "method": "switch_off", "params": {}}),
            )

        self.assertTrue(json.loads(self.client.published[0][1])["ok"])
        print_mock.assert_called_once_with("Power Meter switched OFF by app RPC command-off")

    def test_set_relay_on_acknowledges_the_app_command_and_logs_it(self) -> None:
        with patch("builtins.print") as print_mock:
            self.meter.on_message(
                self.client,
                None,
                FakeMessage(
                    "command-relay-on",
                    {
                        "id": "command-relay-on",
                        "method": "setRelay",
                        "params": {"enabled": True},
                    },
                ),
            )

        response = json.loads(self.client.published[0][1])
        self.assertEqual(response, {"ok": True, "result": {"relay_on": True}})
        print_mock.assert_called_once_with("Power Meter relay ON by app RPC command-relay-on")

    def test_set_relay_off_acknowledges_the_app_command_and_logs_it(self) -> None:
        with patch("builtins.print") as print_mock:
            self.meter.on_message(
                self.client,
                None,
                FakeMessage(
                    "command-relay-off",
                    {
                        "id": "command-relay-off",
                        "method": "setRelay",
                        "params": {"enabled": False},
                    },
                ),
            )

        response = json.loads(self.client.published[0][1])
        self.assertEqual(response, {"ok": True, "result": {"relay_on": False}})
        print_mock.assert_called_once_with("Power Meter relay OFF by app RPC command-relay-off")
