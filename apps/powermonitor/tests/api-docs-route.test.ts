import { afterEach, describe, expect, it } from "vitest";

import { GET } from "../app/api/v1/docs/route";

const originalPlatformBaseUrl = process.env.PLATFORM_BASE_URL;

afterEach(() => {
  if (originalPlatformBaseUrl === undefined) delete process.env.PLATFORM_BASE_URL;
  else process.env.PLATFORM_BASE_URL = originalPlatformBaseUrl;
});

describe("PowerMonitor API documentation route", () => {
  it("redirects to the Platform Swagger documentation", () => {
    process.env.PLATFORM_BASE_URL = "http://platform.example.test:18081";

    const response = GET();

    expect(response.status).toBe(307);
    expect(response.headers.get("location")).toBe("http://platform.example.test:18081/docs/");
  });
});
