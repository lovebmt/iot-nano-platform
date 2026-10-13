import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../../../lib/api-auth";
import { setDevicePower } from "../../../../../../lib/iot-nano";

export async function POST(request: Request, context: { params: Promise<{ deviceId: string }> }) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const body = await request.json() as { enabled?: boolean };
    if (typeof body.enabled !== "boolean") throw new Error("Enabled is required");
    await setDevicePower((await context.params).deviceId, body.enabled);
    return NextResponse.json({ ok: true });
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "platform_unavailable" }, { status: 502 });
  }
}
