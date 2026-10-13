import { NextResponse } from "next/server";
import { signSession, sessionCookieName } from "../../../../lib/auth";
import { verifyLocalAdmin } from "../../../../lib/database";

export async function POST(request: Request) {
  const body = await request.json() as { username?: string; password?: string };
  if (!verifyLocalAdmin(body.username ?? "", body.password ?? "")) return NextResponse.json({ error: "invalid_credentials" }, { status: 401 });
  const response = NextResponse.json({ ok: true });
  response.cookies.set(sessionCookieName(), signSession("admin"), { httpOnly: true, sameSite: "lax", path: "/" });
  return response;
}
