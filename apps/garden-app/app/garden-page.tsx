import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { sessionCookieName, verifySession } from "../lib/auth";
import { gardenSummary } from "../lib/database";
import { GardenDashboard, type GardenSection } from "./garden-dashboard";

export async function GardenPage({ section }: { section: GardenSection }) {
  const cookieStore = await cookies();
  if (!verifySession(cookieStore.get(sessionCookieName())?.value)) redirect("/login");
  return <GardenDashboard summary={gardenSummary()} section={section} />;
}
