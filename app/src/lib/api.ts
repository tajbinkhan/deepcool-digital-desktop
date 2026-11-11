// Backend calls. Outside Tauri (a plain browser, used for layout work) the
// calls return sample data so every page can be previewed.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export interface CpuView { name: string; load: number; mhz: number; temp: number | null; power: number | null }
export interface GpuView {
  index: number; name: string; load: number; mhz: number; temp: number | null; power: number;
  memUsed: number | null; memTotal: number | null;
}
export interface DiskView { name: string; used: number; total: number }
export interface Snapshot {
  cpu: CpuView; gpus: string[]; gpu: GpuView | null;
  memUsed: number; memTotal: number;
  diskRead: number; diskWrite: number; disks: DiskView[];
  netUp: number; netDown: number;
}
export interface SystemInfo {
  hostname: string; os: string; cpu: string; gpus: string[]; motherboard: string | null;
  memoryTotal: number; drives: { model: string; serial: string | null }[];
}
export interface Profile {
  mode: string; secondary: string; rotate: number; fahrenheit: boolean; alarm: boolean;
  zeros: boolean; update: number; gpuid: string; autostart: boolean;
}
export interface Capabilities {
  modes: string[]; secondary: string[]; fahrenheit: boolean; alarm: boolean; rotate: boolean; leadZeros: boolean;
}
export interface Device {
  pid: number; name: string; series: string; category: string; display: string; supported: boolean;
  capabilities: Capabilities | null; running: boolean; error: string | null; profile: Profile;
}
export interface PowerStatus { access: "readable" | "restricted" | "no_sensor"; ruleInstalled: boolean }
export interface Preferences {
  run_in_background: boolean; check_for_updates: boolean; fahrenheit: boolean; language: string; start_at_login: boolean;
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// ---- Sample data for the browser preview ----
const GB = 1024 ** 3;
const mock = {
  gpu: 1,
  power: { access: "restricted", ruleInstalled: false } as PowerStatus,
  prefs: { run_in_background: true, check_for_updates: true, fahrenheit: false, language: "", start_at_login: true } as Preferences,
  profile: { mode: "Device default", secondary: "Device default", rotate: 0, fahrenheit: false, alarm: false, zeros: true, update: 1000, gpuid: "", autostart: true } as Profile,
  snapshot(): Snapshot {
    const jitter = (n: number, r: number) => Math.max(0, Math.round(n + (Math.random() - 0.5) * r));
    const gpus = ["NVIDIA GeForce RTX 5060", "AMD Radeon"];
    return {
      cpu: { name: "AMD Ryzen 7 9800X3D", load: 6, mhz: 5016, temp: 43, power: mock.power.access === "readable" ? 26 : null },
      gpus,
      gpu: { index: mock.gpu, name: gpus[mock.gpu], load: 0, mhz: 600, temp: 37, power: 9, memUsed: 0.2 * GB, memTotal: 1 * GB },
      memUsed: 8 * GB, memTotal: 31.4 * GB,
      diskRead: 14e6, diskWrite: 1.1e6,
      disks: [{ name: "Samsung SSD 990 PRO 1TB", used: 660e9, total: 932e9 }],
      netUp: jitter(4000, 0), netDown: 5000,
    };
  },
};

async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const value: Record<string, () => unknown> = {
    get_snapshot: () => mock.snapshot(),
    select_gpu: () => { mock.gpu = args.index as number; return mock.snapshot(); },
    get_system_info: () => ({
      hostname: "DESKTOP", os: "Ubuntu 24.04 LTS (x86_64) Kernel 6.8.0-generic", cpu: "AMD Ryzen 7 9800X3D",
      gpus: ["NVIDIA GeForce RTX 5060", "AMD Radeon"], motherboard: "GIGABYTE X870 EAGLE WIFI7",
      memoryTotal: 32 * GB, drives: [{ model: "Samsung SSD 990 PRO 1TB", serial: "S0000000000000X" }],
    }),
    list_devices: () => [{
      pid: 10, name: "LD-Series", series: "LD-Series", category: "water_cooler", display: "ld", supported: true,
      capabilities: { modes: ["auto"], secondary: [], fahrenheit: true, alarm: false, rotate: false, leadZeros: true },
      running: true, error: null, profile: mock.profile,
    }],
    save_profile: () => { mock.profile = args.profile as Profile; return null; },
    start_display: () => null,
    stop_display: () => null,
    get_preferences: () => mock.prefs,
    set_preferences: () => { mock.prefs = args.preferences as Preferences; return null; },
    app_version: () => "0.16.0",
    open_website: () => null,
    cpu_power_status: () => mock.power,
    set_cpu_power: () => {
      const allow = args.allow as boolean;
      mock.power = { access: allow ? "readable" : "restricted", ruleInstalled: allow };
      return mock.power;
    },
  };
  if (!(cmd in value)) throw new Error(`Unknown command ${cmd}`);
  return value[cmd]() as T;
}

export function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return inTauri ? tauriInvoke<T>(cmd, args) : mockInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (inTauri) return tauriListen<T>(event, (e) => handler(e.payload));
  if (event !== "telemetry") return () => {};
  const id = setInterval(() => handler(mock.snapshot() as T), 1000);
  return () => clearInterval(id);
}
