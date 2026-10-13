import { NextResponse } from "next/server";
import { requireGardenAdmin } from "../../../../lib/api-auth";
import { createZone, listZones } from "../../../../lib/database";

export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  return unauthorized ?? NextResponse.json(listZones());
}
export async function POST(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try { const body = await request.json() as { areaId?: string; name?: string; cropName?: string }; return NextResponse.json(createZone(body.areaId ?? "", body.name ?? "", body.cropName ?? ""), { status: 201 }); }
  catch (error) { return NextResponse.json({ error: error instanceof Error ? error.message : "invalid_request" }, { status: 400 }); }
}
