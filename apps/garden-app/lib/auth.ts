import { createHmac, timingSafeEqual } from "node:crypto";

const cookieName = "garden_app_session";
const secret = process.env.GARDEN_APP_SESSION_SECRET ?? "garden-app-local-development-secret-change-me";

export function sessionCookieName() { return cookieName; }

export function signSession(username: string): string {
  const payload = `${username}:${Date.now()}`;
  const signature = createHmac("sha256", secret).update(payload).digest("base64url");
  return Buffer.from(`${payload}.${signature}`).toString("base64url");
}

export function verifySession(value: string | undefined): boolean {
  if (!value) return false;
  try {
    const decoded = Buffer.from(value, "base64url").toString();
    const [payload, signature] = decoded.split(".");
    const expected = createHmac("sha256", secret).update(payload).digest("base64url");
    return timingSafeEqual(Buffer.from(signature), Buffer.from(expected)) && payload.startsWith("admin:");
  } catch { return false; }
}

export function isGardenAdminRequest(request: Request): boolean {
  const cookies = request.headers.get("cookie") ?? "";
  const value = cookies
    .split(";")
    .map((entry) => entry.trim())
    .find((entry) => entry.startsWith(`${cookieName}=`))
    ?.slice(cookieName.length + 1);
  return verifySession(value);
}
