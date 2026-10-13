// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PowerMonitorDashboard } from "../components/powermonitor-dashboard";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("PowerMonitor dashboard", () => {
  it("shows Add asset only when the current user can create assets", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/user-capabilities") {
        return json({ capabilities: ["create_assets"] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    expect(await screen.findByRole("button", { name: "Add asset" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add device" })).toBeNull();
  });

  it("shows Add device only when the current user can claim devices", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/user-capabilities") {
        return json({ capabilities: ["claim_devices"] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    expect(await screen.findByRole("button", { name: "Add device" })).toBeTruthy();
  });

  it("creates an asset with the profile selected in the explorer action", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets" && init?.method !== "POST") return json({ items: [] });
      if (path === "/api/v1/asset-profiles") return json([{ id: "farm-v1", name: "Power Farm" }]);
      if (path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["create_assets"] });
      if (path === "/api/v1/assets" && init?.method === "POST") {
        return json({ asset_profile_id: "farm-v1", id: "farm-1", name: "North farm" });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    fireEvent.click(await screen.findByRole("button", { name: "Add asset" }));
    fireEvent.change(screen.getByLabelText("Asset name"), { target: { value: "North farm" } });
    fireEvent.change(screen.getByLabelText("Asset profile"), { target: { value: "farm-v1" } });
    fireEvent.click(screen.getByRole("button", { name: "Create asset" }));

    await waitFor(() => expect(fetcher).toHaveBeenCalledWith(
      "/api/v1/assets",
      expect.objectContaining({
        body: JSON.stringify({
          asset_profile_id: "farm-v1",
          metadata: {},
          name: "North farm",
          parent_asset_id: null,
        }),
        method: "POST",
      }),
    ));
  });

  it("uses the selected asset as the parent for new assets and claimed devices", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets") return json({ items: [{ id: "site-1", name: "Main site" }] });
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["claim_devices", "create_assets"] });
      if (path === "/api/v1/assets/site-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry?asset_id=site-1&aggregate=asset&")) return json({ items: [] });
      if (path === "/api/v1/devices/claim" && init?.method === "POST") {
        return json({ device_id: "meter-2", display_name: "New meter" });
      }
      if (path === "/api/v1/devices/meter-2" && init?.method === "PATCH") {
        return json({ asset_id: "site-1", device_id: "meter-2", display_name: "New meter" });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialAssetId="site-1" />);

    fireEvent.click(await screen.findByRole("button", { name: "Add asset" }));
    expect((screen.getByLabelText("Parent asset") as HTMLSelectElement).value).toBe("site-1");
    fireEvent.click(within(screen.getByRole("region", { name: "Add asset" })).getByRole("button", { name: "Cancel" }));

    fireEvent.click(screen.getByRole("button", { name: "Add device" }));
    const addDevice = screen.getByRole("region", { name: "Add device" });
    fireEvent.change(within(addDevice).getByLabelText("Serial number"), { target: { value: "METER-2" } });
    fireEvent.change(within(addDevice).getByLabelText("Pairing code"), { target: { value: "pair-2" } });
    fireEvent.click(within(addDevice).getByRole("button", { name: "Add device" }));

    await waitFor(() => expect(fetcher).toHaveBeenCalledWith(
      "/api/v1/devices/meter-2",
      expect.objectContaining({
        body: JSON.stringify({ asset_id: "site-1" }),
        method: "PATCH",
      }),
    ));
  });

  it("shows a temporary device under the selected asset until its configuration is completed", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets") return json({ items: [{ id: "site-1", name: "Main site" }] });
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["claim_devices"] });
      if (path === "/api/v1/assets/site-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry?asset_id=site-1&aggregate=asset&")) return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialAssetId="site-1" />);

    fireEvent.click(await screen.findByRole("button", { name: "Add device" }));

    expect((await screen.findByRole("button", { name: "Select Configuring device" })).getAttribute("aria-current"))
      .toBe("true");

    fireEvent.click(within(screen.getByRole("region", { name: "Add device" })).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: "Select Configuring device" })).toBeNull());
  });

  it("shows a temporary asset under the selected asset until its configuration is completed", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets") return json({ items: [{ id: "site-1", name: "Main site" }] });
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["create_assets"] });
      if (path === "/api/v1/assets/site-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry?asset_id=site-1&aggregate=asset&")) return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialAssetId="site-1" />);

    fireEvent.click(await screen.findByRole("button", { name: "Add asset" }));

    expect((await screen.findByRole("button", { name: "Select Configuring asset" })).getAttribute("aria-current"))
      .toBe("true");

    fireEvent.click(within(screen.getByRole("region", { name: "Add asset" })).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: "Select Configuring asset" })).toBeNull());
  });

  it("uses the selected device's parent asset when creating a sibling asset", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") return json({ items: [{ asset_id: "site-1", id: "meter-1", name: "Main meter" }] });
      if (path === "/api/v1/assets") return json({ items: [{ id: "site-1", name: "Main site" }] });
      if (path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") return json({ items: [] });
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["create_assets"] });
      if (path === "/api/v1/devices/meter-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry/meter-1?")) return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialDeviceId="meter-1" />);

    fireEvent.click(await screen.findByRole("button", { name: "Add asset" }));

    expect((screen.getByLabelText("Parent asset") as HTMLSelectElement).value).toBe("site-1");
  });

  it("places compact create actions above the tree and removes redundant controls", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["claim_devices", "create_assets"] });
      if (path === "/api/v1/resource-invitations") return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    const addAsset = await screen.findByRole("button", { name: "Add asset" });
    expect(addAsset.className).toContain("explorer-create-button");
    expect(addAsset.closest(".explorer")).not.toBeNull();
    expect(screen.queryByRole("tab")).toBeNull();
    expect(screen.queryByRole("button", { name: "1h" })).toBeNull();
    expect(screen.queryByRole("button", { name: "24h" })).toBeNull();
    expect(screen.queryByRole("button", { name: "7d" })).toBeNull();
  });

  it("allows only one asset or device creation flow at a time", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: ["claim_devices", "create_assets"] });
      if (path === "/api/v1/resource-invitations") return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    const addAsset = await screen.findByRole("button", { name: "Add asset" });
    const addDevice = screen.getByRole("button", { name: "Add device" });
    fireEvent.click(addAsset);
    expect(addDevice.hasAttribute("disabled")).toBe(true);
    fireEvent.click(within(screen.getByRole("region", { name: "Add asset" })).getByRole("button", { name: "Cancel" }));

    fireEvent.click(addDevice);
    expect(addAsset.hasAttribute("disabled")).toBe(true);
  });

  it("shows fleet overview only from Home, not while a resource is selected", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") return json({ items: [{ id: "meter-1", name: "Main meter" }] });
      if (path === "/api/v1/assets") return json({ items: [{ id: "site-1", name: "Main site" }] });
      if (path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") return json({ items: [] });
      if (path === "/api/v1/asset-profiles" || path === "/api/v1/device-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: [] });
      if (path === "/api/v1/devices/meter-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry/meter-1?")) return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    fireEvent.click(await screen.findByRole("button", { name: "Select Main meter" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Fleet summary" })).toBeNull());

    fireEvent.click(screen.getByRole("button", { name: "Home" }));

    const overview = await screen.findByRole("region", { name: "Fleet summary" });
    expect(within(overview).getByText("Devices")).toBeTruthy();
    expect(within(overview).getByText("Online")).toBeTruthy();
    expect(within(overview).getByText("Assets")).toBeTruthy();
    expect(within(overview).getByText("Alerts")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Telemetry" })).toBeNull();
    expect(screen.queryByRole("heading", { name: "Send command" })).toBeNull();
  });

  it("uses a fixed top menu with icon actions and no refresh button", () => {
    const markup = renderToStaticMarkup(<PowerMonitorDashboard />);

    expect(markup).toContain('class="app-menu"');
    expect(markup).not.toContain('class="explorer-heading"');
    expect(markup).toContain('aria-label="Invitations"');
    expect(markup).toContain('aria-label="Profile"');
    expect(markup).not.toContain('aria-label="Login"');
    expect(markup).not.toContain('aria-label="Logout"');
    expect(markup).not.toContain('aria-label="Refresh Power Monitor"');
  });

  it("puts the API documentation link beside Sign out in the profile menu", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/user-capabilities") return json({ capabilities: [] });
      if (path === "/api/v1/resource-invitations") return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    fireEvent.click(screen.getByRole("button", { name: "Profile" }));
    const menu = await screen.findByRole("menu", { name: "Profile menu" });
    expect(within(menu).getByRole("menuitem", { name: "Sign out" })).toBeTruthy();
    const docs = within(menu).getByRole("menuitem", { name: "Open API docs" });
    expect(docs.getAttribute("href")).toBe("/api/v1/docs");
    expect(docs.getAttribute("target")).toBe("_blank");
  });

  it("keeps unassigned devices inside the single collapsible Root", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        return json({ items: [{
          device_id: "meter-1",
          display_name: "Unassigned meter",
          effective_permission: "manager",
        }] });
      }
      if (path === "/api/v1/assets") {
        return json({ items: [{ id: "zone-1", name: "Main zone", effective_permission: "manager" }] });
      }
      if (path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") return json({ items: [] });
      if (path === "/api/v1/device-profiles" || path === "/api/v1/asset-profiles") return json([]);
      if (path === "/api/v1/user-capabilities") return json({ capabilities: [] });
      if (path === "/api/v1/devices/meter-1/live-view") return json({ charts: [], profile: null });
      if (path.startsWith("/api/v1/telemetry/meter-1?")) return json({ items: [] });
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    const root = await screen.findByRole("button", { name: "Collapse root" });
    expect(screen.getByRole("button", { name: "Select Unassigned meter" })).toBeTruthy();

    fireEvent.click(root);

    expect(screen.getByRole("button", { name: "Expand root" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Select Unassigned meter" })).toBeNull();
  });

  it("renders a concise workspace header, telemetry, alerts, and command surfaces", () => {
    const markup = renderToStaticMarkup(<PowerMonitorDashboard initialDeviceId="meter-1" />);

    expect(markup).toContain("Power Monitor");
    expect(markup).toContain("Root");
    expect(markup).toContain('aria-label="Resource path"');
    expect(markup).not.toContain("Operational energy view");
    expect(markup).not.toContain("Asset explorer");
    expect(markup).toContain("Telemetry");
    expect(markup).not.toContain('aria-label="Fleet summary"');
    expect(markup).toContain("Send command");
    expect(markup).toContain('aria-label="Profile"');
  });

  it("shows pending invitations from the workspace header", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [{
          id: "invite-1",
          permission: "viewer",
          resource_id: "meter-1",
          resource_kind: "device",
          resource_name: "Main meter",
          sender_username: "owner-a",
        }] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    const button = await screen.findByRole("button", { name: "Invitations" });
    fireEvent.click(button);

    expect(screen.getByText("Main meter")).toBeTruthy();
    expect(screen.getByText(/owner-a shared viewer access to this device/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Accept invitation for Main meter" })).toBeTruthy();
  });

  it("keeps authorized resources usable when an existing session lacks invitation scope", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        return json({ items: [{ id: "meter-1", name: "Main meter" }] });
      }
      if (path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        return new Response(JSON.stringify({ message: "The current scope cannot access this resource." }), {
          status: 403,
        });
      }
      if (path.startsWith("/api/v1/telemetry/meter-1?")) {
        return json({ items: [] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialDeviceId="meter-1" />);

    expect(await screen.findByRole("heading", { level: 1, name: "Main meter" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Invitations" })).toBeTruthy();
  });

  it("shows a manager device profile as read-only and never patches it", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        return json({ items: [{
          device_id: "meter-1",
          device_profile_id: "meter-v1",
          display_name: "Main meter",
          effective_permission: "manager",
        }] });
      }
      if (path === "/api/v1/assets" || path === "/api/v1/alerts" || path === "/api/v1/asset-profiles") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/device-profiles") {
        return json([
          { id: "meter-v1", name: "Power Meter v1" },
          { id: "inverter-v1", name: "Solar Inverter v1" },
        ]);
      }
      if (path === "/api/v1/devices/meter-1/live-view") {
        return json({
          profile: { id: "meter-v1", name: "Power Meter v1" },
          charts: [{
            aggregation: "last",
            label: "Configured device power",
            metric: "power_w",
            unit: "W",
          }],
        });
      }
      if (path.startsWith("/api/v1/telemetry/meter-1?")) {
        return json({ items: [] });
      }
      if (path === "/api/v1/devices/meter-1/alert-rules") {
        return json({ items: [] });
      }
      if (path === "/api/v1/devices/meter-1" && init?.method === "PATCH") {
        return json({ device_id: "meter-1", device_profile_id: "meter-v1", display_name: "Main meter" });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialDeviceId="meter-1" />);

    expect(await screen.findByText("Configured device power")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Profile" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Edit selected resource" }));
    expect(screen.queryByLabelText("Device profile")).toBeNull();
    expect((await screen.findAllByText("Power Meter v1")).length).toBeGreaterThan(1);
    fireEvent.click(screen.getByRole("button", { name: "Save configuration" }));

    await waitFor(() => expect(fetcher).toHaveBeenCalledWith(
      "/api/v1/devices/meter-1",
      expect.objectContaining({
        body: JSON.stringify({ asset_id: null, display_name: "Main meter" }),
        method: "PATCH",
      }),
    ));
  });

  it("shows a manager asset profile as read-only and never patches it", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets") {
        return json({ items: [{
          asset_profile_id: "farm-v1",
          effective_permission: "manager",
          id: "farm-1",
          name: "Main farm",
        }] });
      }
      if (path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/asset-profiles") {
        return json([{ id: "farm-v1", name: "Power Farm v1" }]);
      }
      if (path === "/api/v1/device-profiles") {
        return json([]);
      }
      if (path === "/api/v1/assets/farm-1/live-view") {
        return json({
          profile: { id: "farm-v1", name: "Power Farm v1" },
          charts: [{ aggregation: "sum", label: "Farm demand", metric: "power_w", unit: "W" }],
        });
      }
      if (path.startsWith("/api/v1/telemetry?asset_id=farm-1&")) {
        return json({ items: [] });
      }
      if (path === "/api/v1/assets/farm-1" && init?.method === "PATCH") {
        return json({ asset_profile_id: "farm-v1", id: "farm-1", name: "Main farm" });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialAssetId="farm-1" />);

    fireEvent.click(await screen.findByRole("button", { name: "Profile" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Edit selected resource" }));
    expect(screen.queryByLabelText("Asset profile")).toBeNull();
    expect((await screen.findAllByText("Power Farm v1")).length).toBeGreaterThan(1);
    fireEvent.click(screen.getByRole("button", { name: "Save configuration" }));

    await waitFor(() => expect(fetcher).toHaveBeenCalledWith(
      "/api/v1/assets/farm-1",
      expect.objectContaining({
        body: JSON.stringify({ name: "Main farm", parent_asset_id: null }),
        method: "PATCH",
      }),
    ));
  });

  it("keeps profile assignment unavailable to a viewer", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices") {
        return json({ items: [{
          device_id: "meter-1",
          display_name: "Main meter",
          effective_permission: "viewer",
        }] });
      }
      if (path === "/api/v1/assets" || path === "/api/v1/alerts" || path === "/api/v1/resource-invitations") {
        return json({ items: [] });
      }
      if (path === "/api/v1/devices/meter-1/live-view") {
        return json({ profile: { id: "meter-v1", name: "Power Meter v1" }, charts: [] });
      }
      if (path.startsWith("/api/v1/telemetry/meter-1?")) {
        return json({ items: [] });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard initialDeviceId="meter-1" />);

    await screen.findByRole("heading", { level: 1, name: "Main meter" });
    expect(screen.queryByLabelText("Device profile")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Profile" }));
    expect(screen.queryByRole("menuitem", { name: "Edit selected resource" })).toBeNull();
    expect(fetcher).not.toHaveBeenCalledWith(
      "/api/v1/tenant-profile/profiles?kind=device",
      expect.anything(),
    );
    expect(fetcher).toHaveBeenCalledWith(
      "/api/v1/devices/meter-1/live-view",
      expect.objectContaining({ credentials: "same-origin" }),
    );
  });

  it("refreshes invitations after a decline to invalidate an in-flight workspace read", async () => {
    let invitationReads = 0;
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const path = typeof input === "string" ? input : input.toString();
      if (path === "/api/v1/devices" || path === "/api/v1/assets" || path === "/api/v1/alerts") {
        return json({ items: [] });
      }
      if (path === "/api/v1/resource-invitations") {
        invitationReads += 1;
        return json({ items: invitationReads === 1 ? [{
          id: "invite-1",
          permission: "viewer",
          resource_id: "meter-1",
          resource_kind: "device",
          resource_name: "Main meter",
          sender_username: "owner-a",
        }] : [] });
      }
      if (path === "/api/v1/resource-invitations/invite-1/cancel") {
        return new Response(null, { status: 204 });
      }
      throw new Error("Unexpected BFF request: " + path);
    });
    vi.stubGlobal("fetch", fetcher);

    render(<PowerMonitorDashboard />);

    fireEvent.click(await screen.findByRole("button", { name: "Invitations" }));
    fireEvent.click(screen.getByRole("button", { name: "Decline invitation for Main meter" }));

    await waitFor(() => expect(invitationReads).toBe(2));
    expect(screen.queryByText("Main meter")).toBeNull();
  });

});

function json(payload: unknown): Response {
  return new Response(JSON.stringify(payload), { status: 200 });
}
