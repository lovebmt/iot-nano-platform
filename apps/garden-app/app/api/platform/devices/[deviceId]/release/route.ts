import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../../../lib/api-auth";
import { forgetZoneDevice } from "../../../../../../lib/database";
import { releaseDeviceOwnership } from "../../../../../../lib/iot-nano";

export async function POST(request: Request, context: { params: Promise<{ deviceId: string }> }) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const deviceId = (await context.params).deviceId;
    await releaseDeviceOwnership(deviceId);
    forgetZoneDevice(deviceId);
    return new NextResponse(null, { status: 204 });
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "platform_unavailable" }, { status: 502 });
  }
}
