import Database from "better-sqlite3";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { randomBytes, scryptSync, timingSafeEqual } from "node:crypto";
import { randomUUID } from "node:crypto";

const databasePath =
  process.env.GARDEN_APP_DATABASE_PATH?.trim() ||
  join(process.cwd(), "data", "garden-app.sqlite");
mkdirSync(dirname(databasePath), { recursive: true });

export const database = new Database(databasePath);

database.exec(`
  PRAGMA foreign_keys = ON;
  CREATE TABLE IF NOT EXISTS app_users (username TEXT PRIMARY KEY, password_hash TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS farms (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS areas (id TEXT PRIMARY KEY, farm_id TEXT NOT NULL REFERENCES farms(id) ON DELETE CASCADE, name TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS crop_zones (id TEXT PRIMARY KEY, area_id TEXT NOT NULL REFERENCES areas(id) ON DELETE CASCADE, name TEXT NOT NULL, crop_name TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS zone_devices (iot_device_id TEXT PRIMARY KEY, serial_number TEXT, zone_id TEXT REFERENCES crop_zones(id) ON DELETE CASCADE, device_kind TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS production_records (id TEXT PRIMARY KEY, zone_id TEXT NOT NULL REFERENCES crop_zones(id) ON DELETE CASCADE, harvested_at TEXT NOT NULL, amount_kg REAL NOT NULL, note TEXT NOT NULL DEFAULT '');
  CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
`);

const zoneDeviceColumn = database.prepare("PRAGMA table_info(zone_devices)").all() as { name: string; notnull: number }[];
if (zoneDeviceColumn.find((column) => column.name === "zone_id")?.notnull) {
  database.exec(`
    ALTER TABLE zone_devices RENAME TO zone_devices_legacy;
    CREATE TABLE zone_devices (iot_device_id TEXT PRIMARY KEY, zone_id TEXT REFERENCES crop_zones(id) ON DELETE CASCADE, device_kind TEXT NOT NULL);
    INSERT INTO zone_devices (iot_device_id, zone_id, device_kind) SELECT iot_device_id, zone_id, device_kind FROM zone_devices_legacy;
    DROP TABLE zone_devices_legacy;
  `);
}
if (!zoneDeviceColumn.find((column) => column.name === "serial_number")) database.exec("ALTER TABLE zone_devices ADD COLUMN serial_number TEXT");

function passwordHash(password: string): string {
  const salt = randomBytes(16).toString("hex");
  return `${salt}:${scryptSync(password, salt, 32).toString("hex")}`;
}

database.prepare("INSERT OR IGNORE INTO app_users (username, password_hash) VALUES ('admin', ?)").run(passwordHash("admin"));

export function verifyLocalAdmin(username: string, password: string): boolean {
  const user = database.prepare("SELECT password_hash FROM app_users WHERE username = ?").get(username) as { password_hash: string } | undefined;
  if (!user) return false;
  const [salt, expected] = user.password_hash.split(":");
  const actual = scryptSync(password, salt, 32).toString("hex");
  return timingSafeEqual(Buffer.from(actual, "hex"), Buffer.from(expected, "hex"));
}

export function appSetting(key: string): string | undefined {
  return (database.prepare("SELECT value FROM app_settings WHERE key = ?").get(key) as { value: string } | undefined)?.value;
}

export function setAppSetting(key: string, value: string): void {
  database.prepare("INSERT INTO app_settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value").run(key, value);
}

export type GardenSummary = { farms: number; areas: number; zones: number; devices: number; productionKg: number };

export function gardenSummary(): GardenSummary {
  const count = (table: string) => (database.prepare(`SELECT count(*) AS value FROM ${table}`).get() as { value: number }).value;
  const production = database.prepare("SELECT coalesce(sum(amount_kg), 0) AS value FROM production_records").get() as { value: number };
  return { farms: count("farms"), areas: count("areas"), zones: count("crop_zones"), devices: count("zone_devices"), productionKg: production.value };
}

export type Farm = { id: string; name: string };
export type Area = { id: string; farmId: string; name: string };
export type CropZone = { id: string; areaId: string; name: string; cropName: string };

export function listFarms(): Farm[] { return database.prepare("SELECT id, name FROM farms ORDER BY name").all() as Farm[]; }
export function listAreas(farmId?: string): Area[] { return database.prepare(`SELECT id, farm_id AS farmId, name FROM areas ${farmId ? "WHERE farm_id = ?" : ""} ORDER BY name`).all(...(farmId ? [farmId] : [])) as Area[]; }
export function listZones(areaId?: string): CropZone[] { return database.prepare(`SELECT id, area_id AS areaId, name, crop_name AS cropName FROM crop_zones ${areaId ? "WHERE area_id = ?" : ""} ORDER BY name`).all(...(areaId ? [areaId] : [])) as CropZone[]; }
export function createFarm(name: string): Farm { const farm = { id: randomUUID(), name: name.trim() }; if (!farm.name) throw new Error("Farm name is required"); database.prepare("INSERT INTO farms (id, name, created_at) VALUES (?, ?, ?)").run(farm.id, farm.name, new Date().toISOString()); return farm; }
export function createArea(farmId: string, name: string): Area { const area = { id: randomUUID(), farmId, name: name.trim() }; if (!area.farmId || !area.name) throw new Error("Farm and area name are required"); database.prepare("INSERT INTO areas (id, farm_id, name) VALUES (?, ?, ?)").run(area.id, area.farmId, area.name); return area; }
export function createZone(areaId: string, name: string, cropName: string): CropZone { const zone = { id: randomUUID(), areaId, name: name.trim(), cropName: cropName.trim() }; if (!zone.areaId || !zone.name || !zone.cropName) throw new Error("Area, zone and crop are required"); database.prepare("INSERT INTO crop_zones (id, area_id, name, crop_name) VALUES (?, ?, ?, ?)").run(zone.id, zone.areaId, zone.name, zone.cropName); return zone; }
function updateName(table: "farms" | "areas", id: string, name: string): void { const next = name.trim(); if (!next) throw new Error("Name is required"); if (!database.prepare(`UPDATE ${table} SET name = ? WHERE id = ?`).run(next, id).changes) throw new Error("Record not found"); }
function deleteById(table: "farms" | "areas" | "crop_zones", id: string): void { if (!database.prepare(`DELETE FROM ${table} WHERE id = ?`).run(id).changes) throw new Error("Record not found"); }
export function updateFarm(id: string, name: string) { updateName("farms", id, name); }
export function deleteFarm(id: string) { deleteById("farms", id); }
export function updateArea(id: string, name: string) { updateName("areas", id, name); }
export function deleteArea(id: string) { deleteById("areas", id); }
export function updateZone(id: string, name: string, cropName: string) { const nextName = name.trim(), nextCrop = cropName.trim(); if (!nextName || !nextCrop) throw new Error("Zone and crop are required"); if (!database.prepare("UPDATE crop_zones SET name = ?, crop_name = ? WHERE id = ?").run(nextName, nextCrop, id).changes) throw new Error("Record not found"); }
export function deleteZone(id: string) { deleteById("crop_zones", id); }
export type ZoneDevice = { iotDeviceId: string; serialNumber: string | null; zoneId: string | null; deviceKind: string };
export function listZoneDevices(): ZoneDevice[] { return database.prepare("SELECT iot_device_id AS iotDeviceId, serial_number AS serialNumber, zone_id AS zoneId, device_kind AS deviceKind FROM zone_devices ORDER BY iot_device_id").all() as ZoneDevice[]; }
export function createZoneDevice(iotDeviceId: string, serialNumber: string | null, zoneId: string | null, deviceKind: string) { const id = iotDeviceId.trim(), kind = deviceKind.trim(); if (!id || !kind) throw new Error("Device ID and kind are required"); database.prepare("INSERT INTO zone_devices (iot_device_id, serial_number, zone_id, device_kind) VALUES (?, ?, ?, ?)").run(id, serialNumber?.trim() || null, zoneId || null, kind); }
export function updateZoneDevice(iotDeviceId: string, serialNumber: string | null, zoneId: string | null, deviceKind: string) {
  const id = iotDeviceId.trim(), kind = deviceKind.trim();
  if (!id || !kind) throw new Error("Device ID and profile are required");
  database.prepare(`INSERT INTO zone_devices (iot_device_id, serial_number, zone_id, device_kind) VALUES (?, ?, ?, ?)
    ON CONFLICT(iot_device_id) DO UPDATE SET serial_number = coalesce(excluded.serial_number, zone_devices.serial_number), zone_id = excluded.zone_id, device_kind = excluded.device_kind`)
    .run(id, serialNumber?.trim() || null, zoneId || null, kind);
}
export function deleteZoneDevice(iotDeviceId: string) { if (!database.prepare("DELETE FROM zone_devices WHERE iot_device_id = ?").run(iotDeviceId).changes) throw new Error("Device not found"); }
export function forgetZoneDevice(iotDeviceId: string) { database.prepare("DELETE FROM zone_devices WHERE iot_device_id = ?").run(iotDeviceId); }

export type ProductionRecord = { id: string; zoneId: string; zoneName: string; harvestedAt: string; amountKg: number; note: string };
export function listProductionRecords(): ProductionRecord[] {
  return database.prepare(`SELECT production_records.id, zone_id AS zoneId, crop_zones.name AS zoneName,
    harvested_at AS harvestedAt, amount_kg AS amountKg, note
    FROM production_records JOIN crop_zones ON crop_zones.id = production_records.zone_id
    ORDER BY harvested_at DESC, production_records.id DESC`).all() as ProductionRecord[];
}
export function createProductionRecord(zoneId: string, harvestedAt: string, amountKg: number, note: string): ProductionRecord {
  const amount = Number(amountKg);
  const date = harvestedAt.trim();
  if (!zoneId || !date || !Number.isFinite(amount) || amount < 0) throw new Error("Zone, harvest date, and a non-negative amount are required");
  const record = { id: randomUUID(), zoneId, harvestedAt: date, amountKg: amount, note: note.trim() };
  database.prepare("INSERT INTO production_records (id, zone_id, harvested_at, amount_kg, note) VALUES (?, ?, ?, ?, ?)")
    .run(record.id, record.zoneId, record.harvestedAt, record.amountKg, record.note);
  const stored = database.prepare(`SELECT production_records.id, zone_id AS zoneId, crop_zones.name AS zoneName,
    harvested_at AS harvestedAt, amount_kg AS amountKg, note
    FROM production_records JOIN crop_zones ON crop_zones.id = production_records.zone_id WHERE production_records.id = ?`)
    .get(record.id) as ProductionRecord | undefined;
  if (!stored) throw new Error("Production record not found");
  return stored;
}
export function deleteProductionRecord(id: string) { if (!database.prepare("DELETE FROM production_records WHERE id = ?").run(id).changes) throw new Error("Production record not found"); }
export function updateProductionRecord(id: string, zoneId: string, harvestedAt: string, amountKg: number, note: string): ProductionRecord {
  const amount = Number(amountKg);
  const date = harvestedAt.trim();
  if (!zoneId || !date || !Number.isFinite(amount) || amount < 0) throw new Error("Zone, harvest date, and a non-negative amount are required");
  if (!database.prepare("UPDATE production_records SET zone_id = ?, harvested_at = ?, amount_kg = ?, note = ? WHERE id = ?").run(zoneId, date, amount, note.trim(), id).changes) throw new Error("Production record not found");
  const record = listProductionRecords().find((entry) => entry.id === id);
  if (!record) throw new Error("Production record not found");
  return record;
}
