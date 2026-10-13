import { appSetting, setAppSetting } from "./database";

const baseUrl = process.env.IOT_NANO_BASE_URL ?? "http://127.0.0.1:18081";
const configuredPat = process.env.IOT_NANO_USER_PAT?.trim();
if (configuredPat) setAppSetting("iot_nano_user_pat", configuredPat);
const userPat = configuredPat || appSetting("iot_nano_user_pat");

export async function platformRequest(
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  if (!userPat) throw new Error("IOT_NANO_USER_PAT is required in .env.local");
  return fetch(new URL(path, baseUrl), {
    ...init,
    headers: {
      ...init.headers,
      authorization: `Bearer ${userPat}`,
      accept: "application/json",
    },
    cache: "no-store",
  });
}

export function deviceClaimFailureMessage(status: number): string {
  if (status === 403) return "PAT user không có quyền claim device này.";
  if (status === 404)
    return "Không tìm thấy Serial Number trong tenant hiện tại.";
  if (status === 409)
    return "Pairing Code không đúng, đã hết hạn, đã dùng hoặc device đã được claim.";
  if (status === 429)
    return "Bạn vừa tạo Pairing Code. Hãy chờ hết thời gian cooldown rồi thử lại.";
  return `Không thể claim device (IoT Nano trả về ${status}).`;
}

export async function claimPlatformDevice(
  serialNumber: string,
  pairingCode: string,
): Promise<string> {
  const response = await platformRequest("/api/v1/devices/claim", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ serial_number: serialNumber, code: pairingCode }),
  });
  if (!response.ok) throw new Error(deviceClaimFailureMessage(response.status));
  const device = (await response.json()) as { device_id?: string };
  if (!device.device_id) throw new Error("Claim response omitted device_id");
  return device.device_id;
}

export async function listPlatformDevices(): Promise<unknown> {
  const [devicesResponse, profilesResponse] = await Promise.all([
    platformRequest("/api/v1/devices"),
    platformRequest("/api/v1/device-profiles"),
  ]);
  if (!devicesResponse.ok || !profilesResponse.ok)
    throw new Error("IoT Nano devices or Device Profiles are unavailable");
  const devices = (await devicesResponse.json()) as PlatformDevicePage;
  const profiles = (await profilesResponse.json()) as PlatformDeviceProfile[];
  return {
    ...devices,
    items: (devices.items ?? []).map((device) => ({
      ...device,
      device_profile_name:
        profiles.find((profile) => profile.id === device.device_profile_id)
          ?.name ?? null,
    })),
  };
}

export type DeviceTelemetryPoint = {
  event_at?: string;
  received_at?: string;
  measurements?: Record<string, unknown>;
};

export type PlatformAlert = {
  id: string;
  rule_id: string;
  rule_name: string;
  severity: string;
  device_id: string;
  status: string;
  condition_started_at: string;
  opened_at?: string | null;
  resolved_at?: string | null;
  acknowledged_at?: string | null;
  acknowledged_by?: string | null;
  last_value?: number | null;
  updated_at: string;
};

export type PlatformAlertRuleInput = {
  name: string;
  enabled: boolean;
  metric_key: string;
  rule_type: "event_threshold" | "window_average";
  comparison: "gt" | "gte" | "lt" | "lte";
  threshold: number;
  window_seconds: number | null;
  for_seconds: number;
  resolve_after_seconds: number;
  reopen_grace_seconds: number;
  hysteresis: number | null;
  severity: "info" | "warning" | "critical";
  reminder_interval_seconds: number;
};

export type PlatformAlertRule = PlatformAlertRuleInput & {
  id: string;
  device_id: string;
  updated_at: string;
};

export async function listPlatformAlertRules(): Promise<PlatformAlertRule[]> {
  const devices = (await listPlatformDevices()) as PlatformDevicePage;
  const rules = await Promise.all(
    (devices.items ?? []).map(async (device) => {
      const response = await platformRequest(
        `/api/v1/devices/${encodeURIComponent(device.device_id)}/alert-rules`,
      );
      if (!response.ok)
        throw new Error(`Device alert rules request failed with ${response.status}`);
      const payload = (await response.json()) as { items?: PlatformAlertRule[] };
      return payload.items ?? [];
    }),
  );
  return rules.flat();
}

export async function createPlatformAlertRule(
  deviceId: string,
  input: PlatformAlertRuleInput,
): Promise<PlatformAlertRule> {
  const response = await platformRequest(
    `/api/v1/devices/${encodeURIComponent(deviceId)}/alert-rules`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    },
  );
  if (!response.ok)
    throw new Error(`Device alert rule create failed with ${response.status}`);
  return response.json() as Promise<PlatformAlertRule>;
}

export async function updatePlatformAlertRule(
  deviceId: string,
  ruleId: string,
  input: PlatformAlertRuleInput,
): Promise<PlatformAlertRule> {
  const response = await platformRequest(
    `/api/v1/devices/${encodeURIComponent(deviceId)}/alert-rules/${encodeURIComponent(ruleId)}`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(input),
    },
  );
  if (!response.ok)
    throw new Error(`Device alert rule update failed with ${response.status}`);
  return response.json() as Promise<PlatformAlertRule>;
}

export async function archivePlatformAlertRule(
  deviceId: string,
  ruleId: string,
): Promise<void> {
  const response = await platformRequest(
    `/api/v1/devices/${encodeURIComponent(deviceId)}/alert-rules/${encodeURIComponent(ruleId)}`,
    { method: "DELETE" },
  );
  if (!response.ok)
    throw new Error(`Device alert rule delete failed with ${response.status}`);
}

export async function createPlatformAlertRules(
  deviceIds: string[],
  input: PlatformAlertRuleInput,
): Promise<PlatformAlertRule[]> {
  const created: PlatformAlertRule[] = [];
  try {
    for (const deviceId of deviceIds) {
      const rule = await createPlatformAlertRule(deviceId, input);
      created.push({ ...rule, device_id: deviceId });
    }
    return created;
  } catch (error) {
    for (const rule of created.reverse()) {
      try {
        await archivePlatformAlertRule(rule.device_id, rule.id);
      } catch {
        // Keep rolling back the remaining rules; the original create error is actionable.
      }
    }
    throw error;
  }
}

export async function listPlatformAlerts(): Promise<PlatformAlert[]> {
  const response = await platformRequest("/api/v1/alerts?limit=100");
  if (!response.ok)
    throw new Error(`Platform alerts request failed with ${response.status}`);
  const payload = (await response.json()) as { items?: PlatformAlert[] };
  return payload.items ?? [];
}

export async function acknowledgePlatformAlert(alertId: string): Promise<PlatformAlert> {
  const response = await platformRequest(
    `/api/v1/alerts/${encodeURIComponent(alertId)}/acknowledge`,
    { method: "POST" },
  );
  if (!response.ok)
    throw new Error(`Platform alert acknowledgement failed with ${response.status}`);
  return response.json() as Promise<PlatformAlert>;
}

async function listDeviceTelemetryInWindow(
  deviceId: string,
  windowMs: number,
): Promise<DeviceTelemetryPoint[]> {
  const to = new Date();
  const from = new Date(to.getTime() - windowMs);
  const response = await platformRequest(
    `/api/v1/telemetry?device_id=${encodeURIComponent(deviceId)}&from=${encodeURIComponent(from.toISOString())}&to=${encodeURIComponent(to.toISOString())}&limit=100`,
  );
  if (!response.ok)
    throw new Error(`Device telemetry request failed with ${response.status}`);
  const payload = (await response.json()) as { items?: DeviceTelemetryPoint[] };
  return (payload.items ?? []).sort(
    (left, right) =>
      Date.parse(left.event_at ?? left.received_at ?? "") -
      Date.parse(right.event_at ?? right.received_at ?? ""),
  );
}

export function listDeviceTelemetry(
  deviceId: string,
): Promise<DeviceTelemetryPoint[]> {
  return listDeviceTelemetryInWindow(deviceId, 24 * 60 * 60 * 1000);
}

export function listRecentDeviceTelemetry(
  deviceId: string,
): Promise<DeviceTelemetryPoint[]> {
  return listDeviceTelemetryInWindow(deviceId, 15 * 60 * 1000);
}

export async function setDevicePower(
  deviceId: string,
  enabled: boolean,
): Promise<void> {
  const response = await platformRequest(
    `/api/v1/devices/${encodeURIComponent(deviceId)}/commands`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": crypto.randomUUID(),
      },
      body: JSON.stringify({
        method: "setRelay",
        params: { enabled },
        mode: "two_way",
      }),
    },
  );
  if (!response.ok)
    throw new Error(`Device control failed with ${response.status}`);
}

export async function releaseDeviceOwnership(deviceId: string): Promise<void> {
  const response = await platformRequest(
    `/api/v1/devices/${encodeURIComponent(deviceId)}/ownership/release`,
    {
      method: "POST",
    },
  );
  if (!response.ok)
    throw new Error(
      response.status === 403
        ? "Chỉ owner hiện tại mới có thể bỏ quyền sở hữu device."
        : `Không thể bỏ quyền sở hữu device (${response.status}).`,
    );
}

type PlatformDevicePage = {
  items?: {
    device_id: string;
    serial_number?: string;
    device_profile_id?: string | null;
  }[];
};
type PlatformDeviceProfile = { id: string; name: string };

export async function resolveDeviceProfile(
  deviceReference: string,
): Promise<{
  deviceId: string;
  serialNumber: string | null;
  profileId: string;
  profileName: string;
}> {
  const [devicesResponse, profilesResponse] = await Promise.all([
    platformRequest("/api/v1/devices"),
    platformRequest("/api/v1/device-profiles"),
  ]);
  if (!devicesResponse.ok || !profilesResponse.ok)
    throw new Error("IoT Nano device profiles are unavailable");
  const devices = (await devicesResponse.json()) as PlatformDevicePage;
  const device = devices.items?.find(
    (item) =>
      item.device_id === deviceReference ||
      item.serial_number === deviceReference,
  );
  if (!device?.device_profile_id)
    throw new Error("Device has no IoT Nano Device Profile");
  const profiles = (await profilesResponse.json()) as PlatformDeviceProfile[];
  const profile = profiles.find((item) => item.id === device.device_profile_id);
  return {
    deviceId: device.device_id,
    serialNumber: device.serial_number ?? null,
    profileId: device.device_profile_id,
    profileName: profile?.name ?? device.device_profile_id,
  };
}
