import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../lib/api-auth";
import { listZoneDevices } from "../../../../lib/database";
import {
  createPlatformAlertRules,
  listPlatformAlertRules,
  type PlatformAlertRuleInput,
} from "../../../../lib/iot-nano";

export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    return NextResponse.json(await listPlatformAlertRules());
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "platform_unavailable" },
      { status: 502 },
    );
  }
}

export async function POST(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const body = (await request.json()) as {
      zoneId?: string;
      input?: PlatformAlertRuleInput;
    };
    const deviceIds = [
      ...new Set(
        listZoneDevices()
          .filter((device) => device.zoneId === body.zoneId)
          .map((device) => device.iotDeviceId),
      ),
    ];
    if (!body.zoneId || !body.input || deviceIds.length === 0)
      throw new Error("Vùng trồng phải có ít nhất một device");
    return NextResponse.json(
      await createPlatformAlertRules(deviceIds, body.input),
      { status: 201 },
    );
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "invalid_request" },
      { status: 400 },
    );
  }
}
