// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PowerMonitorDashboard } from "../components/powermonitor-dashboard";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("PowerMonitor refresh ownership", () => {
  it("uses the fixed one-hour range when a terminal command refreshes telemetry", async () => {
    let deviceRead = 0;
    const telemetryPaths: string[] = [];
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        deviceRead += 1;
        return json({ items: [device(deviceRead > 1)] });
      }
      if (path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path.startsWith("/api/v1/telemetry/device-1?")) {
        telemetryPaths.push(path);
        return json({ items: [{ at: "2026-09-13T10:00:00Z", power_w: 42 }] });
      }
      if (path === "/api/v1/devices/device-1/commands") {
        return json({ id: "command-1", state: "queued" });
      }
      if (path === "/api/v1/commands/command-1") {
        return json({ id: "command-1", state: "responded" });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialDeviceId="device-1" />);
    await screen.findByRole("button", { name: "Relay on" });

    vi.useFakeTimers();
    fireEvent.click(screen.getByRole("button", { name: "Relay on" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
      await Promise.resolve();
    });
    vi.useRealTimers();

    await waitFor(() => {
      expect(deviceRead).toBe(2);
      const lastPath = telemetryPaths.at(-1);
      expect(lastPath).toBeDefined();
      expect(telemetryDuration(lastPath as string)).toBe(60 * 60 * 1_000);
    });
  });

  it("polls visible telemetry every five seconds and refreshes when the tab becomes visible", async () => {
    let telemetryReads = 0;
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        return json({ items: [device(false)] });
      }
      if (path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path.startsWith("/api/v1/telemetry/device-1?")) {
        telemetryReads += 1;
        return json({ items: [{ at: "2026-09-13T10:00:00Z", power_w: telemetryReads }] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(false);

    vi.useFakeTimers();
    render(<PowerMonitorDashboard initialDeviceId="device-1" />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(telemetryReads).toBe(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(telemetryReads).toBe(2);

    hidden.mockReturnValue(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(telemetryReads).toBe(2);

    hidden.mockReturnValue(false);
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
      await Promise.resolve();
    });
    expect(telemetryReads).toBe(3);
  });
});

function device(on: boolean) {
  return {
    brightness_pct: on ? 75 : 20,
    capabilities: ["switch", "brightness"],
    id: "device-1",
    name: "Main meter",
    online: true,
    switch_state: on,
  };
}

function json(payload: unknown): Response {
  return new Response(JSON.stringify(payload), { status: 200 });
}

function telemetryDuration(path: string): number {
  const url = new URL(path, "https://powermonitor.example.test");
  return new Date(url.searchParams.get("to") as string).getTime()
    - new Date(url.searchParams.get("from") as string).getTime();
}
