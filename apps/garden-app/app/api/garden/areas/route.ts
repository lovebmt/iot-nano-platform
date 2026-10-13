import { NextResponse } from "next/server";
import { requireGardenAdmin } from "../../../../lib/api-auth";
import { createArea, listAreas } from "../../../../lib/database";

export async function GET(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  return unauthorized ?? NextResponse.json(listAreas());
}
export async function POST(request: Request) {
  const unauthorized = requireGardenAdmin(request);
  if (unauthorized) return unauthorized;
  try { const body = await request.json() as { farmId?: string; name?: string }; return NextResponse.json(createArea(body.farmId ?? "", body.name ?? ""), { status: 201 }); }
  catch (error) { return NextResponse.json({ error: error instanceof Error ? error.message : "invalid_request" }, { status: 400 }); }
}
