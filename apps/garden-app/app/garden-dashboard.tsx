"use client";

import { FormEvent, useEffect, useRef, useState } from "react";

type Farm = { id: string; name: string };
type Area = { id: string; farmId: string; name: string };
type Zone = { id: string; areaId: string; name: string; cropName: string };
type GardenDevice = {
  iotDeviceId: string;
  serialNumber: string | null;
  zoneId: string | null;
  deviceKind: string;
};
type PlatformDevice = {
  device_id: string;
  serial_number?: string | null;
  device_profile_id?: string | null;
  device_profile_name?: string | null;
};
type ProductionRecord = {
  id: string;
  zoneId: string;
  zoneName: string;
  harvestedAt: string;
  amountKg: number;
  note: string;
};
type PlatformAlertRule = {
  id: string;
  name: string;
  device_id: string;
  metric_key: string;
  rule_type: "event_threshold" | "window_average";
  comparison: "gt" | "gte" | "lt" | "lte";
  threshold: number;
  enabled: boolean;
  window_seconds: number | null;
  for_seconds: number;
  resolve_after_seconds: number;
  reopen_grace_seconds: number;
  hysteresis: number | null;
  severity: "info" | "warning" | "critical";
  reminder_interval_seconds: number;
};
type PlatformAlert = {
  id: string;
  rule_name: string;
  severity: string;
  device_id: string;
  status: string;
  opened_at?: string | null;
  acknowledged_at?: string | null;
  last_value?: number | null;
  updated_at: string;
};
type DeviceTelemetryPoint = {
  event_at?: string;
  received_at?: string;
  measurements?: Record<string, unknown>;
};
export type GardenSection = "zones" | "devices" | "production" | "alerts";

export function toggleDevicePanel(
  openDeviceId: string | null,
  deviceId: string,
): string | null {
  return openDeviceId === deviceId ? null : deviceId;
}

export function buildTelemetryPolyline(values: number[]): string {
  if (!values.length) return "";
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  return values
    .map(
      (value, index) =>
        `${4 + (index / Math.max(values.length - 1, 1)) * 92},${96 - ((value - min) / span) * 92}`,
    )
    .join(" ");
}

export function openIncidentCount(
  incidents: Array<{ status: string; acknowledged_at?: string | null }>,
): number {
  return incidents.filter(
    (incident) => incident.status === "open" && !incident.acknowledged_at,
  ).length;
}

export function buildPlatformAlertRuleInput(input: {
  name: string;
  metricKey: string;
  comparison: string;
  threshold: number;
}) {
  return {
    name: input.name,
    enabled: true,
    metric_key: input.metricKey,
    rule_type: "event_threshold" as const,
    comparison: input.comparison,
    threshold: input.threshold,
    window_seconds: null,
    for_seconds: 0,
    resolve_after_seconds: 300,
    reopen_grace_seconds: 3600,
    hysteresis: null,
    severity: "warning" as const,
    reminder_interval_seconds: 86400,
  };
}

function TelemetryChart({ points }: { points: DeviceTelemetryPoint[] }) {
  const detectedKeys = [
    ...new Set(
      points.flatMap((point) =>
        Object.entries(point.measurements ?? {})
          .filter(([, value]) => typeof value === "number")
          .map(([key]) => key),
      ),
    ),
  ];
  const [manualKeys, setManualKeys] = useState<string[]>([]);
  const [manualKey, setManualKey] = useState("");
  const [chartKeyError, setChartKeyError] = useState("");
  const keys = [...new Set([...detectedKeys, ...manualKeys])];
  const [selectedKeys, setSelectedKeys] = useState<string[]>(() =>
    detectedKeys.slice(0, 4),
  );
  useEffect(
    () =>
      setSelectedKeys((current) =>
        current
          .filter((key) => keys.includes(key))
          .concat(
            keys
              .filter((key) => !current.includes(key))
              .slice(0, Math.max(0, 4 - current.length)),
          )
          .slice(0, 4),
      ),
    [points, manualKeys],
  );
  const palette = ["#356e4d", "#1769e0", "#b26d00", "#7556b7"];
  const toggleKey = (key: string) =>
    setSelectedKeys((current) =>
      current.includes(key)
        ? current.filter((item) => item !== key)
        : current.length < 4
          ? [...current, key]
          : current,
    );
  const addManualKey = () => {
    const key = manualKey.trim();
    if (!key) return;
    if (!keys.includes(key) && selectedKeys.length >= 4) {
      setChartKeyError("Chỉ chọn tối đa 4 keys. Bỏ một key đang chọn trước.");
      return;
    }
    setManualKeys((current) =>
      current.includes(key) ? current : [...current, key],
    );
    setSelectedKeys((current) =>
      current.includes(key) || current.length >= 4
        ? current
        : [...current, key],
    );
    setChartKeyError("");
    setManualKey("");
  };
  const removeManualKey = (key: string) => {
    setManualKeys((current) => current.filter((item) => item !== key));
    setSelectedKeys((current) => current.filter((item) => item !== key));
  };
  const series = selectedKeys.map((key, color) => {
    const samples = points
      .map((point) => point.measurements?.[key])
      .filter((value): value is number => typeof value === "number")
      .slice(-40);
    if (!samples.length)
      return { key, color: palette[color], min: null, max: null, polyline: "" };
    const min = Math.min(...samples), max = Math.max(...samples);
    return {
      key,
      color: palette[color],
      min,
      max,
      polyline: buildTelemetryPolyline(samples),
    };
  });
  const firstTimestamp = points.at(0)?.event_at ?? points.at(0)?.received_at;
  const lastTimestamp = points.at(-1)?.event_at ?? points.at(-1)?.received_at;
  return (
    <div className="garden-chart">
      <div className="garden-chart-heading">
        <strong>Telemetry · 24 giờ</strong>
        <span>Chọn tối đa 4 keys</span>
      </div>
      <div className="garden-chart-manual">
        <input
          value={manualKey}
          onChange={(event) => setManualKey(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              addManualKey();
            }
          }}
          placeholder="Nhập telemetry key"
        />
        <button onClick={addManualKey}>Thêm key</button>
      </div>
      {chartKeyError && (
        <p className="garden-chart-key-error">{chartKeyError}</p>
      )}
      <div className="garden-chart-keys">
        {keys.map((key) => {
          const selected = selectedKeys.includes(key);
          return (
            <span className="garden-chart-key" key={key}>
              <button
                className={selected ? "is-selected" : ""}
                onClick={() => toggleKey(key)}
                disabled={!selected && selectedKeys.length >= 4}
              >
                {selected ? "✓ " : ""}
                {key}
              </button>
              {manualKeys.includes(key) && (
                <button
                  className="garden-chart-key-remove"
                  onClick={() => removeManualKey(key)}
                  aria-label={`Remove ${key}`}
                >
                  ×
                </button>
              )}
            </span>
          );
        })}
      </div>
      {keys.length === 0 ? (
        <p className="garden-chart-empty">Chưa có telemetry số trong 24 giờ.</p>
      ) : (
        <>
          <svg
            viewBox="0 0 100 100"
            preserveAspectRatio="none"
            aria-label="Device telemetry chart"
          >
            {series.map(
              (item) =>
                item.polyline && (
                  <polyline
                    key={item.key}
                    points={item.polyline}
                    style={{ stroke: item.color }}
                  />
                ),
            )}
          </svg>
          <div className="garden-chart-axis">
            <span>X · {firstTimestamp ?? "—"}</span>
            <strong>X: timestamp · Y: normalized value per key</strong>
            <span>{lastTimestamp ?? "—"}</span>
          </div>
          <div className="garden-chart-legend">
            {series.map((item) => (
              <span key={item.key}>
                <i style={{ background: item.color }} />
                {item.key}:{" "}
                {item.min === null
                  ? "chưa có dữ liệu"
                  : `${item.min.toFixed(3)} — ${item.max?.toFixed(3)}`}
              </span>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

async function request<T>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = response.status === 204 ? null : await response.json();
  if (!response.ok) throw new Error(data?.error ?? "Request failed");
  return data as T;
}

export function GardenDashboard({
  summary,
  section,
}: {
  summary: {
    farms: number;
    zones: number;
    devices: number;
    productionKg: number;
  };
  section: GardenSection;
}) {
  const [farms, setFarms] = useState<Farm[]>([]);
  const [areas, setAreas] = useState<Area[]>([]);
  const [zones, setZones] = useState<Zone[]>([]);
  const [devices, setDevices] = useState<GardenDevice[]>([]);
  const [platformDevices, setPlatformDevices] = useState<PlatformDevice[]>([]);
  const [production, setProduction] = useState<ProductionRecord[]>([]);
  const [alertRules, setAlertRules] = useState<PlatformAlertRule[]>([]);
  const [incidents, setIncidents] = useState<PlatformAlert[]>([]);
  const [telemetry, setTelemetry] = useState<
    Record<string, DeviceTelemetryPoint[]>
  >({});
  const [telemetryUpdatedAt, setTelemetryUpdatedAt] = useState<
    Record<string, string>
  >({});
  const [message, setMessage] = useState("");
  const [deviceMessage, setDeviceMessage] = useState("");
  const [selectedTreeNode, setSelectedTreeNode] = useState<string | null>(null);
  const [zonesEditMode, setZonesEditMode] = useState(false);
  const [openDeviceId, setOpenDeviceId] = useState<string | null>(null);
  const [editingProduction, setEditingProduction] =
    useState<ProductionRecord | null>(null);
  const productionForm = useRef<HTMLFormElement>(null);

  const loadIncidents = async () => {
    try {
      setIncidents(await request<PlatformAlert[]>("/api/platform/alerts"));
    } catch {
      setIncidents([]);
    }
  };

  const load = async () => {
    const [
      nextFarms,
      nextAreas,
      nextZones,
      nextDevices,
      nextProduction,
    ] = await Promise.all([
      request<Farm[]>("/api/garden/farms"),
      request<Area[]>("/api/garden/areas"),
      request<Zone[]>("/api/garden/zones"),
      request<GardenDevice[]>("/api/garden/devices"),
      request<ProductionRecord[]>("/api/garden/production"),
    ]);
    setFarms(nextFarms);
    setAreas(nextAreas);
    setZones(nextZones);
    setDevices(nextDevices);
    setProduction(nextProduction);
    try {
      const platform = await request<{ items?: PlatformDevice[] }>(
        "/api/platform/devices",
      );
      setPlatformDevices(platform.items ?? []);
    } catch {
      setPlatformDevices([]);
    }
    try {
      setAlertRules(await request<PlatformAlertRule[]>("/api/platform/alert-rules"));
    } catch {
      setAlertRules([]);
    }
  };
  useEffect(() => {
    void load();
    void loadIncidents();
    const interval = window.setInterval(() => void loadIncidents(), 10_000);
    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    if (!openDeviceId) return;
    const interval = window.setInterval(
      () => void loadTelemetry(openDeviceId, true),
      5_000,
    );
    return () => window.clearInterval(interval);
  }, [openDeviceId]);

  const create =
    (path: string, payload: (form: FormData) => unknown) =>
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const form = event.currentTarget;
      try {
        await request(path, "POST", payload(new FormData(form)));
        form.reset();
        await load();
        setMessage("Đã lưu.");
      } catch (error) {
        setMessage(error instanceof Error ? error.message : "Không thể lưu.");
      }
    };
  const remove = async (kind: string, id: string) => {
    if (!window.confirm("Xóa mục này và dữ liệu phụ thuộc?")) return;
    try {
      await request(`/api/garden/${kind}/${encodeURIComponent(id)}`, "DELETE");
      await load();
      setMessage("Đã xóa.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Không thể xóa.");
    }
  };
  const editName = async (
    kind: "farms" | "areas" | "zones",
    item: Farm | Area | Zone,
  ) => {
    const name = window.prompt("Tên", item.name);
    if (!name) return;
    try {
      if (kind === "zones") {
        const cropName = window.prompt("Cây trồng", (item as Zone).cropName);
        if (!cropName) return;
        await request(`/api/garden/zones/${item.id}`, "PUT", {
          name,
          cropName,
        });
      } else await request(`/api/garden/${kind}/${item.id}`, "PUT", { name });
      await load();
      setMessage("Đã cập nhật.");
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể cập nhật.",
      );
    }
  };
  const updateDevice = async (device: GardenDevice, zoneId: string | null) => {
    try {
      await request(
        `/api/garden/devices/${encodeURIComponent(device.iotDeviceId)}`,
        "PUT",
        {
          serialNumber: device.serialNumber,
          zoneId,
          deviceKind: device.deviceKind,
        },
      );
      await load();
      setMessage("Đã cập nhật device.");
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể cập nhật device.",
      );
    }
  };
  const claimDevice = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    setDeviceMessage("");
    try {
      await request("/api/garden/devices", "POST", {
        serialNumber: new FormData(form).get("serialNumber"),
        pairingCode: new FormData(form).get("pairingCode"),
      });
      form.reset();
      await load();
      setMessage("Đã claim device.");
    } catch (error) {
      const next =
        error instanceof Error ? error.message : "Không thể claim device.";
      setDeviceMessage(next);
      setMessage(next);
    }
  };
  const toggleAlertRule = async (rule: PlatformAlertRule) => {
    try {
      await request(`/api/platform/alert-rules/${rule.device_id}/${rule.id}`, "PUT", {
        ...rule,
        enabled: !rule.enabled,
      });
      await load();
      setMessage("Đã cập nhật rule.");
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể cập nhật rule.",
      );
    }
  };
  const createPlatformAlertRule = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    const zoneId = String(values.get("zoneId") ?? "");
    const metricKey = String(values.get("metric") ?? "").trim();
    const comparison = String(values.get("operator") ?? "");
    const threshold = Number(values.get("threshold"));
    const zone = zones.find((item) => item.id === zoneId);
    const zoneDevices = displayDevices.filter((item) => item.zoneId === zoneId);
    if (!zoneId || zoneDevices.length === 0 || !metricKey || !Number.isFinite(threshold)) {
      setMessage("Chọn vùng có device và nhập metric/ngưỡng hợp lệ.");
      return;
    }
    try {
      await request("/api/platform/alert-rules", "POST", {
        zoneId,
        input: buildPlatformAlertRuleInput({
          name: `Garden · ${zone?.name ?? zoneId} · ${metricKey} ${comparison} ${threshold}`,
          metricKey,
          comparison,
          threshold,
        }),
      });
      form.reset();
      await load();
      setMessage(`Đã tạo rule trên IoT Nano cho ${zoneDevices.length} device.`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Không thể tạo rule.");
    }
  };
  const removePlatformAlertRule = async (rule: PlatformAlertRule) => {
    if (!window.confirm("Xóa rule này khỏi IoT Nano?")) return;
    try {
      await request(`/api/platform/alert-rules/${rule.device_id}/${rule.id}`, "DELETE");
      await load();
      setMessage("Đã xóa rule khỏi IoT Nano.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Không thể xóa rule.");
    }
  };
  const acknowledgeIncident = async (incident: PlatformAlert) => {
    try {
      await request(
        `/api/platform/alerts/${encodeURIComponent(incident.id)}/acknowledge`,
        "POST",
      );
      await loadIncidents();
      setMessage("Đã xác nhận incident.");
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể xác nhận incident.",
      );
    }
  };
  const loadTelemetry = async (deviceId: string, recent = false) => {
    try {
      const points = await request<DeviceTelemetryPoint[]>(
        `/api/platform/devices/${encodeURIComponent(deviceId)}/telemetry${recent ? "?window=recent" : ""}`,
      );
      setTelemetry((current) => ({ ...current, [deviceId]: points }));
      setTelemetryUpdatedAt((current) => ({
        ...current,
        [deviceId]: (() => {
          const latest = points.at(-1);
          const recordedAt = latest?.event_at ?? latest?.received_at;
          return recordedAt
            ? new Date(recordedAt).toLocaleTimeString()
            : "chưa có dữ liệu";
        })(),
      }));
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể tải telemetry.",
      );
    }
  };
  const setPower = async (deviceId: string, enabled: boolean) => {
    try {
      await request(
        `/api/platform/devices/${encodeURIComponent(deviceId)}/power`,
        "POST",
        { enabled },
      );
      setMessage(`${deviceId}: đã gửi lệnh ${enabled ? "BẬT" : "TẮT"}.`);
    } catch (error) {
      setMessage(
        error instanceof Error
          ? error.message
          : "Không thể gửi lệnh điều khiển.",
      );
    }
  };
  const releaseDevice = async (
    deviceId: string,
    serialNumber: string | null,
  ) => {
    if (
      !window.confirm(
        `Bỏ quyền sở hữu ${serialNumber ?? deviceId}? Device sẽ thành unassigned và có thể được user khác claim.`,
      )
    )
      return;
    try {
      await request(
        `/api/platform/devices/${encodeURIComponent(deviceId)}/release`,
        "POST",
      );
      await load();
      setMessage("Device đã được bỏ quyền sở hữu.");
    } catch (error) {
      setMessage(
        error instanceof Error
          ? error.message
          : "Không thể bỏ quyền sở hữu device.",
      );
    }
  };
  const editProduction = (record: ProductionRecord) => {
    setEditingProduction(record);
    const form = productionForm.current;
    if (!form) return;
    form.elements.namedItem("zoneId") &&
      ((form.elements.namedItem("zoneId") as HTMLSelectElement).value =
        record.zoneId);
    form.elements.namedItem("harvestedAt") &&
      ((form.elements.namedItem("harvestedAt") as HTMLInputElement).value =
        record.harvestedAt);
    form.elements.namedItem("amountKg") &&
      ((form.elements.namedItem("amountKg") as HTMLInputElement).value = String(
        record.amountKg,
      ));
    form.elements.namedItem("note") &&
      ((form.elements.namedItem("note") as HTMLInputElement).value =
        record.note);
  };
  const saveProduction = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const body = {
      zoneId: new FormData(form).get("zoneId"),
      harvestedAt: new FormData(form).get("harvestedAt"),
      amountKg: Number(new FormData(form).get("amountKg")),
      note: new FormData(form).get("note"),
    };
    try {
      if (editingProduction)
        await request(
          `/api/garden/production/${editingProduction.id}`,
          "PUT",
          body,
        );
      else await request("/api/garden/production", "POST", body);
      form.reset();
      setEditingProduction(null);
      await load();
      setMessage("Đã lưu sản lượng.");
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Không thể lưu sản lượng.",
      );
    }
  };
  const logout = async () => {
    await fetch("/api/auth/logout", { method: "POST" });
    window.location.assign("/login");
  };
  const maxProduction = Math.max(
    ...production.map((record) => record.amountKg),
    1,
  );
  const displayDevices: GardenDevice[] = [
    ...platformDevices.map((platform) => {
      const local = devices.find(
        (device) =>
          device.iotDeviceId === platform.device_id ||
          (device.serialNumber !== null &&
            device.serialNumber === platform.serial_number),
      );
      return local
        ? {
            ...local,
            iotDeviceId: platform.device_id,
            serialNumber: platform.serial_number ?? local.serialNumber,
            deviceKind: platform.device_profile_name ?? local.deviceKind,
          }
        : {
            iotDeviceId: platform.device_id,
            serialNumber: platform.serial_number ?? null,
            zoneId: null,
            deviceKind:
              platform.device_profile_name ?? "Chưa có Device Profile",
          };
    }),
    ...devices.filter(
      (device) =>
        !platformDevices.some(
          (platform) =>
            platform.device_id === device.iotDeviceId ||
            (device.serialNumber !== null &&
              device.serialNumber === platform.serial_number),
        ),
    ),
  ];
  const activeIncidentCount = openIncidentCount(incidents);
  const activeIncidents = incidents.filter(
    (incident) => incident.status === "open" && !incident.acknowledged_at,
  );

  return (
    <main className="garden-shell">
      <aside className="garden-nav">
        <p className="garden-mark">GARDEN OPS</p>
        <h1>Vườn trồng</h1>
        <nav>
          <a className={section === "zones" ? "active" : ""} href="/zones">
            Phân vùng
          </a>
          <a className={section === "devices" ? "active" : ""} href="/devices">
            Thiết bị riêng
          </a>
          <a
            className={section === "production" ? "active" : ""}
            href="/production"
          >
            Sản lượng
          </a>
          <a className={section === "alerts" ? "active" : ""} href="/alerts">
            Cảnh báo
            {activeIncidentCount > 0 && (
              <span className="garden-alert-badge" aria-label={`${activeIncidentCount} incident đang mở`}>
                {activeIncidentCount}
              </span>
            )}
          </a>
        </nav>
        <section className="garden-side-tree" aria-label="Sơ đồ phân vùng">
          <p>PHÂN VÙNG</p>
          {farms.length === 0 && <span>Chưa có Farm</span>}
          {farms.map((farm) => (
            <article key={farm.id}>
              <strong>{farm.name}</strong>
              {areas
                .filter((area) => area.farmId === farm.id)
                .map((area) => (
                  <div key={area.id}>
                    └ {area.name}
                    {zones
                      .filter((zone) => zone.areaId === area.id)
                      .map((zone) => (
                        <small key={zone.id}>
                          └ {zone.name} · {zone.cropName}
                        </small>
                      ))}
                  </div>
                ))}
            </article>
          ))}
        </section>
      </aside>
      <section className="garden-content">
        <header className="garden-header">
          <div>
            <p className="eyebrow">LOCAL FARM OPERATIONS</p>
            <h2>Dashboard</h2>
            <p>
              Quản lý nghiệp vụ riêng, không dùng relation device-asset của IoT
              Nano.
            </p>
          </div>
          <button className="garden-logout" onClick={() => void logout()}>
            Đăng xuất
          </button>
        </header>
        <div className="garden-stats">
          <article>
            <span>Farms</span>
            <strong>{farms.length || summary.farms}</strong>
          </article>
          <article>
            <span>Vùng trồng</span>
            <strong>{zones.length || summary.zones}</strong>
          </article>
          <article>
            <span>Thiết bị riêng</span>
            <strong>{devices.length || summary.devices}</strong>
          </article>
          <article>
            <span>Sản lượng</span>
            <strong>{summary.productionKg} kg</strong>
          </article>
        </div>
        <p className="garden-message">{message}</p>
        {section === "zones" && (
          <section
            className={`garden-panel garden-zones-panel ${zonesEditMode ? "is-editing" : ""}`}
            id="tree"
          >
            <h3>
              <span>Phân vùng canh tác</span>
              <button
                className="garden-zone-edit-toggle"
                onClick={() => setZonesEditMode((current) => !current)}
              >
                {zonesEditMode ? "Xong" : "Chỉnh sửa"}
              </button>
            </h3>
            <div className="garden-forms">
              <form
                onSubmit={create("/api/garden/farms", (f) => ({
                  name: f.get("name"),
                }))}
              >
                <label>
                  Farm
                  <input name="name" placeholder="Nông trại A" required />
                </label>
                <button>Tạo Farm</button>
              </form>
              <form
                onSubmit={create("/api/garden/areas", (f) => ({
                  farmId: f.get("farmId"),
                  name: f.get("name"),
                }))}
              >
                <label>
                  Khu vực
                  <select name="farmId" required>
                    <option value="">Chọn Farm</option>
                    {farms.map((farm) => (
                      <option key={farm.id} value={farm.id}>
                        {farm.name}
                      </option>
                    ))}
                  </select>
                  <input name="name" placeholder="Khu phía Bắc" required />
                </label>
                <button>Thêm khu vực</button>
              </form>
              <form
                onSubmit={create("/api/garden/zones", (f) => ({
                  areaId: f.get("areaId"),
                  name: f.get("name"),
                  cropName: f.get("cropName"),
                }))}
              >
                <label>
                  Vùng trồng
                  <select name="areaId" required>
                    <option value="">Chọn khu vực</option>
                    {areas.map((area) => (
                      <option key={area.id} value={area.id}>
                        {area.name}
                      </option>
                    ))}
                  </select>
                  <input name="name" placeholder="Lô 01" required />
                  <input name="cropName" placeholder="Cây trồng" required />
                </label>
                <button>Thêm vùng trồng</button>
              </form>
            </div>
            <div className="garden-zone-actions">
              {farms.map((farm) => (
                <article
                  key={farm.id}
                  className={
                    selectedTreeNode === farm.id ? "tree-node-selected" : ""
                  }
                  role="button"
                  tabIndex={0}
                  onClick={() => setSelectedTreeNode(farm.id)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ")
                      setSelectedTreeNode(farm.id);
                  }}
                >
                  <header>
                    <strong>
                      <small>FARM</small>
                      {farm.name}
                    </strong>
                    <span
                      className="tree-actions"
                      onClick={(event) => event.stopPropagation()}
                    >
                      <button onClick={() => void editName("farms", farm)}>
                        Sửa
                      </button>
                      <button onClick={() => void remove("farms", farm.id)}>
                        Xóa
                      </button>
                    </span>
                  </header>
                  {areas
                    .filter((area) => area.farmId === farm.id)
                    .map((area) => (
                      <div
                        key={area.id}
                        className={
                          selectedTreeNode === area.id
                            ? "tree-node-selected"
                            : ""
                        }
                        role="button"
                        tabIndex={0}
                        onClick={(event) => {
                          event.stopPropagation();
                          setSelectedTreeNode(area.id);
                        }}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ")
                            setSelectedTreeNode(area.id);
                        }}
                      >
                        <header>
                          <span>
                            <small>KHU VỰC</small>
                            {area.name}
                          </span>
                          <span
                            className="tree-actions"
                            onClick={(event) => event.stopPropagation()}
                          >
                            <button
                              onClick={() => void editName("areas", area)}
                            >
                              Sửa
                            </button>
                            <button
                              onClick={() => void remove("areas", area.id)}
                            >
                              Xóa
                            </button>
                          </span>
                        </header>
                        {zones
                          .filter((zone) => zone.areaId === area.id)
                          .map((zone) => (
                            <section className="garden-zone-node" key={zone.id}>
                              <p
                                className={
                                  selectedTreeNode === zone.id
                                    ? "tree-node-selected"
                                    : ""
                                }
                                role="button"
                                tabIndex={0}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  setSelectedTreeNode(zone.id);
                                }}
                                onKeyDown={(event) => {
                                  if (
                                    event.key === "Enter" ||
                                    event.key === " "
                                  )
                                    setSelectedTreeNode(zone.id);
                                }}
                              >
                                <span>
                                  <small>VÙNG TRỒNG</small>
                                  {zone.name} <em>{zone.cropName}</em>
                                </span>
                                <span
                                  className="tree-actions"
                                  onClick={(event) => event.stopPropagation()}
                                >
                                  <button
                                    onClick={() => void editName("zones", zone)}
                                  >
                                    Sửa
                                  </button>
                                  <button
                                    onClick={() =>
                                      void remove("zones", zone.id)
                                    }
                                  >
                                    Xóa
                                  </button>
                                </span>
                              </p>
                              {displayDevices
                                .filter((device) => device.zoneId === zone.id)
                                .map((device) => {
                                  const latest =
                                    telemetry[device.iotDeviceId]?.at(-1);
                                  const pumpOn =
                                    latest?.measurements?.pump_on === true;
                                  const telemetryEntries = Object.entries(
                                    latest?.measurements ?? {},
                                  )
                                    .filter(
                                      ([, value]) =>
                                        typeof value === "number" ||
                                        typeof value === "boolean",
                                    )
                                    .slice(0, 6);
                                  return (
                                    <div
                                      className={`garden-tree-device ${openDeviceId === device.iotDeviceId ? "is-open" : ""}`}
                                      key={device.iotDeviceId}
                                      tabIndex={0}
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        const nextOpenDeviceId = toggleDevicePanel(
                                          openDeviceId,
                                          device.iotDeviceId,
                                        );
                                        setOpenDeviceId(nextOpenDeviceId);
                                        if (nextOpenDeviceId) {
                                          void loadTelemetry(
                                            device.iotDeviceId,
                                            true,
                                          );
                                        }
                                      }}
                                      onKeyDown={(event) => {
                                        if (event.key !== "Enter" && event.key !== " ") return;
                                        event.preventDefault();
                                        event.currentTarget.click();
                                      }}
                                    >
                                      <span
                                        className="garden-device-icon"
                                        aria-hidden="true"
                                      >
                                        ◉
                                      </span>
                                      <div>
                                        <strong>
                                          {device.serialNumber ??
                                            device.iotDeviceId}
                                        </strong>
                                      </div>
                                      <span
                                        className={`garden-device-status ${pumpOn ? "is-on" : "is-off"}`}
                                      >
                                        {pumpOn ? "ON" : "OFF"}
                                      </span>
                                      <section
                                        className="garden-tree-device-actions"
                                        role="dialog"
                                        aria-label="Telemetry and pump controls"
                                      >
                                        <header>
                                          <strong>Telemetry</strong>
                                          <span>
                                            {pumpOn
                                              ? "Bơm đang bật"
                                              : "Bơm đang tắt"}
                                          </span>
                                        </header>
                                        <p className="garden-telemetry-updated">
                                          Cập nhật{" "}
                                          {telemetryUpdatedAt[
                                            device.iotDeviceId
                                          ] ?? "đang tải"}{" "}
                                          · polling 5 giây
                                        </p>
                                        {telemetryEntries.length ? (
                                          <div className="garden-tree-telemetry">
                                            {telemetryEntries.map(
                                              ([key, value]) => (
                                                <span key={key}>
                                                  <small>{key}</small>
                                                  <strong>
                                                    {String(value)}
                                                  </strong>
                                                </span>
                                              ),
                                            )}
                                          </div>
                                        ) : (
                                          <p>Chưa nhận telemetry.</p>
                                        )}
                                        <footer>
                                          <button
                                            onClick={(event) => {
                                              event.stopPropagation();
                                              void setPower(
                                                device.iotDeviceId,
                                                true,
                                              );
                                            }}
                                          >
                                            Bật bơm
                                          </button>
                                          <button
                                            onClick={(event) => {
                                              event.stopPropagation();
                                              void setPower(
                                                device.iotDeviceId,
                                                false,
                                              );
                                            }}
                                          >
                                            Tắt bơm
                                          </button>
                                        </footer>
                                      </section>
                                    </div>
                                  );
                                })}
                            </section>
                          ))}
                      </div>
                    ))}
                </article>
              ))}
            </div>
          </section>
        )}
        {section === "devices" && (
          <section className="garden-panel" id="devices">
            <h3>Thiết bị của user</h3>
            <p>
              Danh sách lấy trực tiếp từ IoT Nano: các device mà user của PAT sở
              hữu hoặc được cấp quyền. Nhập Serial Number và Pairing Code để
              claim device mới; gán vùng trồng là tùy chọn sau đó.
            </p>
            <div className="garden-forms">
              <form onSubmit={claimDevice}>
                <label>
                  Serial Number
                  <input name="serialNumber" placeholder="PLT54YK2E" required />
                  <input
                    name="pairingCode"
                    placeholder="Pairing Code"
                    required
                  />
                </label>
                <button>Claim device</button>
              </form>
            </div>
            {deviceMessage && (
              <p className="garden-inline-error" role="alert">
                {deviceMessage}
              </p>
            )}
            <div className="garden-device-list">
              {displayDevices.map((device) => (
                <article key={device.iotDeviceId}>
                  <div>
                    <strong>{device.serialNumber ?? "Serial chưa có"}</strong>
                    <span>Device ID: {device.iotDeviceId}</span>
                    <span>Device Profile: {device.deviceKind}</span>
                  </div>
                  <label>
                    Gán vùng trồng
                    <select
                      value={device.zoneId ?? ""}
                      onChange={(event) =>
                        void updateDevice(
                          device,
                          event.currentTarget.value || null,
                        )
                      }
                    >
                      <option value="">Chưa gán vùng</option>
                      {zones.map((zone) => (
                        <option key={zone.id} value={zone.id}>
                          {zone.name} · {zone.cropName}
                        </option>
                      ))}
                    </select>
                  </label>
                  <span className="tree-actions">
                    <button
                      onClick={() => void loadTelemetry(device.iotDeviceId)}
                    >
                      Chart 24h
                    </button>
                    <button
                      onClick={() => void setPower(device.iotDeviceId, true)}
                    >
                      Bật
                    </button>
                    <button
                      onClick={() => void setPower(device.iotDeviceId, false)}
                    >
                      Tắt
                    </button>
                    <button
                      onClick={() =>
                        void releaseDevice(
                          device.iotDeviceId,
                          device.serialNumber,
                        )
                      }
                    >
                      Bỏ quyền sở hữu
                    </button>
                  </span>
                  {telemetry[device.iotDeviceId] && (
                    <TelemetryChart points={telemetry[device.iotDeviceId]} />
                  )}
                </article>
              ))}
            </div>
          </section>
        )}
        {section === "production" && (
          <section className="garden-panel" id="production">
            <h3>Sản lượng</h3>
            <p>
              Ghi sản lượng theo vùng trồng; dữ liệu này nằm trong SQLite của
              Garden App.
            </p>
            <div className="garden-forms">
              <form ref={productionForm} onSubmit={saveProduction}>
                <label>
                  {editingProduction ? "Sửa sản lượng" : "Ghi sản lượng"}
                  <select name="zoneId" required>
                    <option value="">Chọn vùng trồng</option>
                    {zones.map((zone) => (
                      <option key={zone.id} value={zone.id}>
                        {zone.name} · {zone.cropName}
                      </option>
                    ))}
                  </select>
                  <input name="harvestedAt" type="date" required />
                  <input
                    name="amountKg"
                    type="number"
                    min="0"
                    step="0.1"
                    placeholder="kg"
                    required
                  />
                  <input name="note" placeholder="Ghi chú" />
                </label>
                <button>
                  {editingProduction ? "Cập nhật sản lượng" : "Lưu sản lượng"}
                </button>
                {editingProduction && (
                  <button
                    type="button"
                    onClick={() => {
                      productionForm.current?.reset();
                      setEditingProduction(null);
                    }}
                  >
                    Hủy
                  </button>
                )}
              </form>
            </div>
            <div className="garden-production-chart">
              {production
                .slice(0, 12)
                .reverse()
                .map((record) => (
                  <div
                    key={`chart-${record.id}`}
                    title={`${record.zoneName}: ${record.amountKg} kg`}
                  >
                    <span
                      style={{
                        height: `${Math.max((record.amountKg / maxProduction) * 100, 4)}%`,
                      }}
                    />
                    <small>{record.harvestedAt.slice(5)}</small>
                  </div>
                ))}
            </div>
            <div className="garden-device-list">
              {production.map((record) => (
                <article key={record.id}>
                  <div>
                    <strong>{record.amountKg} kg</strong>
                    <span>
                      {record.zoneName} · {record.harvestedAt}
                      {record.note ? ` · ${record.note}` : ""}
                    </span>
                  </div>
                  <span className="tree-actions">
                    <button onClick={() => editProduction(record)}>Sửa</button>
                    <button
                      onClick={() => void remove("production", record.id)}
                    >
                      Xóa
                    </button>
                  </span>
                </article>
              ))}
            </div>
          </section>
        )}
        {section === "alerts" && (
          <section className="garden-panel" id="alerts">
            <h3>Incident IoT Nano</h3>
            <p>Tự làm mới mỗi 10 giây. Badge ở menu chỉ tính incident đang mở.</p>
            <div className="garden-device-list garden-incident-list">
              {activeIncidents.length === 0 && (
                <p className="garden-chart-empty">Không có incident cần xử lý.</p>
              )}
              {activeIncidents.map((incident) => (
                <article key={incident.id}>
                  <div>
                    <strong>{incident.rule_name}</strong>
                    <span>
                      Device {incident.device_id} · {incident.severity} · {incident.status}
                    </span>
                    <span>
                      Giá trị cuối: {incident.last_value ?? "—"} · Cập nhật {new Date(incident.updated_at).toLocaleString()}
                    </span>
                  </div>
                  <button onClick={() => void acknowledgeIncident(incident)}>
                    Xác nhận
                  </button>
                </article>
              ))}
            </div>
          </section>
        )}
        {section === "alerts" && (
          <section className="garden-panel" id="alert-rules">
            <h3>Rule cảnh báo IoT Nano</h3>
            <p>Rule được lưu và đánh giá trực tiếp bởi IoT Nano.</p>
            <div className="garden-forms">
              <form
                onSubmit={createPlatformAlertRule}
              >
                <label>
                  Thêm rule IoT Nano
                  <select name="zoneId" required>
                    <option value="">Chọn vùng trồng</option>
                    {zones.map((zone) => (
                      <option key={zone.id} value={zone.id}>
                        {zone.name}
                      </option>
                    ))}
                  </select>
                  <input
                    name="metric"
                    placeholder="temperature_c / soil_moisture"
                    required
                  />
                  <select name="operator">
                    <option value="gt">&gt;</option>
                    <option value="gte">≥</option>
                    <option value="lt">&lt;</option>
                    <option value="lte">≤</option>
                  </select>
                  <input
                    name="threshold"
                    type="number"
                    step="any"
                    placeholder="Ngưỡng"
                    required
                  />
                </label>
                <button>Thêm rule</button>
              </form>
            </div>
            <div className="garden-device-list">
              {alertRules.map((rule) => (
                <article key={rule.id}>
                  <div>
                    <strong>
                      {rule.name}: {rule.metric_key} {rule.comparison}{" "}
                      {rule.threshold}
                    </strong>
                    <span>
                      {displayDevices.find((device) => device.iotDeviceId === rule.device_id)?.serialNumber ?? rule.device_id}
                      {(() => {
                        const zone = zones.find((item) => item.id === displayDevices.find((device) => device.iotDeviceId === rule.device_id)?.zoneId);
                        return zone ? ` · ${zone.name}` : "";
                      })()} · {rule.severity} · {rule.enabled ? "Đang bật" : "Đã tắt"}
                    </span>
                  </div>
                  <span className="tree-actions">
                    <button onClick={() => void toggleAlertRule(rule)}>
                      {rule.enabled ? "Tắt" : "Bật"}
                    </button>
                    <button onClick={() => void removePlatformAlertRule(rule)}>
                      Xóa
                    </button>
                  </span>
                </article>
              ))}
            </div>
          </section>
        )}
      </section>
    </main>
  );
}
