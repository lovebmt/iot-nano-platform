import { NextResponse } from "next/server";
import { requireGardenAdmin } from "../../../../lib/api-auth";
import { createZoneDevice, listZoneDevices } from "../../../../lib/database";
import { claimPlatformDevice, resolveDeviceProfile } from "../../../../lib/iot-nano";
export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  return unauthorized ?? NextResponse.json(listZoneDevices());
}
export async function POST(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try { const body = await request.json() as { serialNumber?: string; pairingCode?: string; zoneId?: string | null }; const deviceId = await claimPlatformDevice(body.serialNumber ?? "", body.pairingCode ?? ""); const profile = await resolveDeviceProfile(deviceId); createZoneDevice(profile.deviceId, profile.serialNumber, body.zoneId ?? null, profile.profileName); return NextResponse.json({ profile }, { status: 201 }); } catch (error) { return NextResponse.json({ error: error instanceof Error ? error.message : "invalid_request" }, { status: 400 }); }
}
