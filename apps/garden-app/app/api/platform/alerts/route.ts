import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../lib/api-auth";
import { listPlatformAlerts } from "../../../../lib/iot-nano";

export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    return NextResponse.json(await listPlatformAlerts());
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "platform_unavailable" },
      { status: 502 },
    );
  }
}
