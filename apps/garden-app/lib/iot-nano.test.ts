import { afterEach, describe, expect, it, vi } from "vitest";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { deviceClaimFailureMessage } from "./iot-nano";

describe("deviceClaimFailureMessage", () => {
  it("explains an unavailable or invalid pairing code", () => {
    expect(deviceClaimFailureMessage(409)).toContain("Pairing Code");
  });

  it("explains a denied user PAT", () => {
    expect(deviceClaimFailureMessage(403)).toContain("PAT user");
  });
});

describe("listRecentDeviceTelemetry", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("requests the latest 15-minute window so a hover popup includes current telemetry", async () => {
    vi.stubEnv("IOT_NANO_USER_PAT", "test-user-pat");
    vi.stubEnv(
      "GARDEN_APP_DATABASE_PATH",
      join(tmpdir(), `garden-app-iot-nano-${process.pid}.sqlite`),
    );
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ items: [] }), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    vi.resetModules();

    const module = (await import("./iot-nano")) as typeof import("./iot-nano") & {
      listRecentDeviceTelemetry: (deviceId: string) => Promise<unknown>;
    };
    await module.listRecentDeviceTelemetry("device-123");

    const requestUrl = new URL(String(fetchMock.mock.calls[0]?.[0]));
    const from = Date.parse(requestUrl.searchParams.get("from") ?? "");
    expect(requestUrl.searchParams.get("device_id")).toBe("device-123");
    expect(Date.now() - from).toBeLessThanOrEqual(15 * 60 * 1000 + 1_000);
  });

  it("archives already-created rules when a later device rule fails", async () => {
    vi.stubEnv("IOT_NANO_USER_PAT", "test-user-pat");
    vi.stubEnv(
      "GARDEN_APP_DATABASE_PATH",
      join(tmpdir(), `garden-app-iot-nano-rollback-${process.pid}.sqlite`),
    );
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ id: "rule-a" }), { status: 201 }),
      )
      .mockResolvedValueOnce(new Response("", { status: 503 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.resetModules();

    const module = (await import("./iot-nano")) as typeof import("./iot-nano") & {
      createPlatformAlertRules: (
        deviceIds: string[],
        input: Record<string, unknown>,
      ) => Promise<unknown>;
    };
    await expect(
      module.createPlatformAlertRules(["device-a", "device-b"], {
        name: "test",
      }),
    ).rejects.toThrow("Device alert rule create failed with 503");

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(String(fetchMock.mock.calls[2]?.[0])).toContain(
      "/api/v1/devices/device-a/alert-rules/rule-a",
    );
    expect(fetchMock.mock.calls[2]?.[1]).toMatchObject({ method: "DELETE" });
  });
});
