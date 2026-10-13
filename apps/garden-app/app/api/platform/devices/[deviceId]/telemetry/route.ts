import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../../../lib/api-auth";
import {
  listDeviceTelemetry,
  listRecentDeviceTelemetry,
} from "../../../../../../lib/iot-nano";

export async function GET(request: Request, context: { params: Promise<{ deviceId: string }> }) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const deviceId = (await context.params).deviceId;
    const recent = new URL(request.url).searchParams.get("window") === "recent";
    return NextResponse.json(
      await (recent
        ? listRecentDeviceTelemetry(deviceId)
        : listDeviceTelemetry(deviceId)),
    );
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "platform_unavailable" }, { status: 502 });
  }
}
