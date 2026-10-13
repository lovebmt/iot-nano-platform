import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../../../lib/api-auth";
import {
  archivePlatformAlertRule,
  type PlatformAlertRuleInput,
  updatePlatformAlertRule,
} from "../../../../../../lib/iot-nano";

export async function PUT(
  request: Request,
  context: { params: Promise<{ deviceId: string; ruleId: string }> },
) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const params = await context.params;
    return NextResponse.json(
      await updatePlatformAlertRule(
        params.deviceId,
        params.ruleId,
        (await request.json()) as PlatformAlertRuleInput,
      ),
    );
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "invalid_request" },
      { status: 400 },
    );
  }
}

export async function DELETE(
  request: Request,
  context: { params: Promise<{ deviceId: string; ruleId: string }> },
) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const params = await context.params;
    await archivePlatformAlertRule(params.deviceId, params.ruleId);
    return new NextResponse(null, { status: 204 });
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "invalid_request" },
      { status: 400 },
    );
  }
}
