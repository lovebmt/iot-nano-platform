import { NextResponse } from "next/server";

import { isGardenAdminRequest } from "./auth";

export function requireGardenAdmin(request: Request): NextResponse | null {
  return isGardenAdminRequest(request)
    ? null
    : NextResponse.json({ error: "unauthorized" }, { status: 401 });
}
