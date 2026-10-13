import { NextResponse } from "next/server";

import { requireGardenAdmin } from "../../../../lib/api-auth";
import { createProductionRecord, listProductionRecords } from "../../../../lib/database";

export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  return unauthorized ?? NextResponse.json(listProductionRecords());
}

export async function POST(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try {
    const body = await request.json() as { zoneId?: string; harvestedAt?: string; amountKg?: number; note?: string };
    return NextResponse.json(
      createProductionRecord(body.zoneId ?? "", body.harvestedAt ?? "", body.amountKg ?? Number.NaN, body.note ?? ""),
      { status: 201 },
    );
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "invalid_request" }, { status: 400 });
  }
}
