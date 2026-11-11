// App-wide state shared by all pages.

import { call, listen, type Device, type PowerStatus, type Preferences, type Snapshot, type UpdateView } from "./api";
import { resolveLang, translate, type Key, type Lang } from "./i18n";

export type Page = "dashboard" | "computer" | "device" | "settings";

export const app = $state({
  page: "dashboard" as Page,
  /** Product ID of the device whose settings are open, if any. */
  detail: null as number | null,
  snapshot: null as Snapshot | null,
  prefs: null as Preferences | null,
  lang: "en" as Lang,
  devices: [] as Device[],
  version: "",
  power: null as PowerStatus | null,
  /** True while the administrator password prompt for CPU power is open. */
  powerBusy: false,
  powerError: null as string | null,
  update: null as UpdateView | null,
});

export function t(key: Key, vars: Record<string, string> = {}): string {
  return translate(app.lang, key, vars);
}

export function go(page: Page) {
  app.page = page;
  app.detail = null;
}

export async function refreshDevices() {
  app.devices = await call<Device[]>("list_devices");
}

export async function savePrefs(changes: Partial<Preferences>) {
  if (!app.prefs) return;
  const next = { ...app.prefs, ...changes };
  app.prefs = next;
  if (changes.language !== undefined) app.lang = resolveLang(next.language);
  await call("set_preferences", { preferences: next });
}

export async function refreshUpdate() {
  app.update = await call<UpdateView>("update_state");
}

export async function refreshPower() {
  app.power = await call<PowerStatus>("cpu_power_status");
}

/** Asks for an administrator password to allow (or restrict) CPU power readings. */
export async function setCpuPower(allow: boolean) {
  if (app.powerBusy) return;
  app.powerBusy = true;
  app.powerError = null;
  try {
    app.power = await call<PowerStatus>("set_cpu_power", { allow });
    app.snapshot = await call<Snapshot>("get_snapshot");
  } catch (e) {
    app.powerError = String(e);
  } finally {
    app.powerBusy = false;
  }
}

export async function init() {
  const [prefs, snapshot, version] = await Promise.all([
    call<Preferences>("get_preferences"),
    call<Snapshot>("get_snapshot"),
    call<string>("app_version"),
  ]);
  app.prefs = prefs;
  app.lang = resolveLang(prefs.language);
  app.snapshot = snapshot;
  app.version = version;
  document.documentElement.lang = app.lang;
  await listen<Snapshot>("telemetry", (s) => (app.snapshot = s));
  await listen("devices-changed", () => refreshDevices());
  await listen("update-changed", () => refreshUpdate());
  await Promise.all([refreshDevices(), refreshPower(), refreshUpdate()]);
  // Devices can be plugged in at any time; check now and then while the window is open.
  setInterval(refreshDevices, 5000);
}
