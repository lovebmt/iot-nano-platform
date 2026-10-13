import { describe, expect, it } from "vitest";

import * as dashboard from "./garden-dashboard";

const interactions = dashboard as typeof dashboard & {
  toggleDevicePanel: (openDeviceId: string | null, deviceId: string) => string | null;
  buildTelemetryPolyline: (values: number[]) => string;
  openIncidentCount: (
    incidents: Array<{ status: string; acknowledged_at?: string | null }>,
  ) => number;
  buildPlatformAlertRuleInput: (input: {
    name: string;
    metricKey: string;
    comparison: string;
    threshold: number;
  }) => Record<string, unknown>;
};

describe("toggleDevicePanel", () => {
  it("opens a device on press and closes it when pressed again", () => {
    expect(interactions.toggleDevicePanel(null, "device-1")).toBe("device-1");
    expect(interactions.toggleDevicePanel("device-1", "device-1")).toBeNull();
  });
});

describe("buildTelemetryPolyline", () => {
  it("places time samples on the X axis and values on the Y axis", () => {
    expect(interactions.buildTelemetryPolyline([10, 20, 30])).toBe(
      "4,96 50,50 96,4",
    );
  });
});

describe("openIncidentCount", () => {
  it("counts only incidents that still need attention", () => {
    expect(
      interactions.openIncidentCount([
        { status: "open" },
        { status: "open", acknowledged_at: "2026-10-12T00:00:00Z" },
        { status: "resolved" },
      ]),
    ).toBe(1);
  });
});

describe("buildPlatformAlertRuleInput", () => {
  it("creates an IoT Nano event rule with durable alert defaults", () => {
    expect(
      interactions.buildPlatformAlertRuleInput({
        name: "Garden · power high",
        metricKey: "power_w",
        comparison: "gt",
        threshold: 700,
      }),
    ).toMatchObject({
      enabled: true,
      metric_key: "power_w",
      rule_type: "event_threshold",
      comparison: "gt",
      threshold: 700,
      severity: "warning",
      resolve_after_seconds: 300,
    });
  });
});
