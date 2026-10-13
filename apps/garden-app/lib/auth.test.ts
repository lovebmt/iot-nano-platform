import { describe, expect, it } from "vitest";

import { isGardenAdminRequest, sessionCookieName, signSession } from "./auth";

describe("isGardenAdminRequest", () => {
  it("rejects a request without the local admin session", () => {
    expect(isGardenAdminRequest(new Request("http://garden.local/api/garden/farms"))).toBe(false);
  });

  it("accepts a request with a signed local admin session", () => {
    const request = new Request("http://garden.local/api/garden/farms", {
      headers: { cookie: `${sessionCookieName()}=${signSession("admin")}` },
    });

    expect(isGardenAdminRequest(request)).toBe(true);
  });
});
