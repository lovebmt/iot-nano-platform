import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../../../lib/api-auth";
import { acknowledgePlatformAlert } from "../../../../../../lib/iot-nano";

export async function POST(
  request: Request,
  context: { params: Promise<{ alertId: string }> },
) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    return NextResponse.json(
      await acknowledgePlatformAlert((await context.params).alertId),
    );
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "platform_unavailable" },
      { status: 502 },
    );
  }
}
