export function GET(): Response {
  const platformBaseUrl = process.env.PLATFORM_BASE_URL;
  if (platformBaseUrl === undefined || platformBaseUrl.length === 0) {
    return new Response("Platform documentation is unavailable", { status: 503 });
  }

  try {
    return Response.redirect(new URL("/docs/", platformBaseUrl), 307);
  } catch {
    return new Response("Platform documentation is unavailable", { status: 503 });
  }
}
