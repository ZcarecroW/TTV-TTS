// Bridge to the Rust side. Outside Tauri (plain browser, `npm run proxy:ui`)
// a demo backend answers instead, so the UI can be developed, tested and
// photographed for the docs without a running proxy.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FriendsApi, InternetStatus, NewInvite } from "@ttv-tts/ui/friends";

export interface ProxyInfo {
  id: string;
  name: string;
  version: string;
  api: number;
  port: number;
  fingerprint: string;
  pairing_open: boolean;
  os: string;
  vcp_reachable: boolean;
  vcp_version: string | null;
}

export interface VcpState {
  url: string;
  running: boolean;
  pid: number | null;
  exe: string | null;
  reachable: boolean;
  key_ok: boolean | null;
  key_required: boolean | null;
  version: string | null;
  error: string | null;
  model_loaded: boolean;
  model_kind: string | null;
  model_name: string | null;
  active_model: string | null;
  queue_size: number;
  generating: boolean;
  hardware: string | null;
  starting: boolean;
  speed: number | null;
  usual_speed: number | null;
}

export interface NetAddress {
  interface: string;
  ip: string;
  prefix: number;
  mac: string;
}

export interface DeviceInfo {
  id: string;
  name: string;
  paired_at: number;
  last_seen: number | null;
  last_address: string | null;
}

export interface UpdateState {
  available: string | null;
  phase: string;
  progress: number | null;
  error: string | null;
  channel: string;
}

export interface ProxyStatus {
  info: ProxyInfo;
  vcp: VcpState;
  addresses: NetAddress[];
  devices: DeviceInfo[];
  connections: number;
  uptime_secs: number;
  update: UpdateState | null;
}

export type KeyMode = "vcp" | "own";

export interface Config {
  id: string;
  name: string;
  port: number;
  discovery: boolean;
  vcp_url: string;
  key_mode: KeyMode;
  vcp_exe: string | null;
  start_vcp: boolean;
  keep_vcp_running: boolean;
  restart_on_hang: boolean;
  restart_when_slow: boolean;
  hang_restart_secs: number;
  http_port: number;
  autostart: boolean;
  update_channel: string;
  auto_update: boolean;
  language: string;
  theme: string;
  setup_done: boolean;
}

export interface Overview {
  status: ProxyStatus;
  config: Config;
  has_vcp_key: boolean;
  fingerprint_short: string;
  data_dir: string;
  log_path: string | null;
  listen_error: string | null;
}

export interface GpuStats {
  name: string;
  vendor: string | null;
  usage_percent: number | null;
  vram_used: number | null;
  vram_total: number | null;
  temperature_c: number | null;
  power_w: number | null;
  processes: { pid: number; name: string; vram: number | null; usage: number | null }[];
}

export interface SystemStats {
  cpu_percent: number;
  mem_used: number;
  mem_total: number;
  gpus: GpuStats[];
}

export interface FirewallStatus {
  managed: boolean;
  rule: { enabled: boolean; action: string; profile: string; program: string } | null;
  rule_ok: boolean;
  allows: number;
  blocks: number;
  profiles: { alias: string; category: string }[];
  public_network_blocked: boolean;
  wfc: boolean;
  third_party: string[];
  firewall_on: string[];
  program: string;
  error: string | null;
}

export interface LogLine {
  t: number;
  level: string;
  message: string;
}

export interface KeyCheck {
  saved: boolean;
  verified: boolean;
  message: string;
}

export interface Pairing {
  code: string;
  secs_left: number;
}

export const isTauri = "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(cmd, args);
  const { demoInvoke } = await import("./demo");
  return demoInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (isTauri) return tauriListen<T>(event, (e) => cb(e.payload));
  const { demoListen } = await import("./demo");
  return demoListen(event, cb as (p: unknown) => void);
}

export const api = {
  overview: () => invoke<Overview>("get_overview"),
  updateSettings: (patch: Partial<Config>) => invoke<Overview>("update_settings", { patch }),
  setVcpKey: (key: string) => invoke<KeyCheck>("set_vcp_key", { key }),
  clearVcpKey: () => invoke<Overview>("clear_vcp_key"),
  createVcpKey: () => invoke<string>("create_vcp_key"),
  detectVcpKey: () => invoke<KeyCheck>("detect_vcp_key"),
  accessKey: () => invoke<string | null>("get_access_key"),
  newAccessKey: () => invoke<string>("new_access_key"),
  setAccessKey: (key: string) => invoke<void>("set_access_key", { key }),
  openPairing: () => invoke<Pairing>("open_pairing"),
  getPairing: () => invoke<Pairing | null>("get_pairing"),
  closePairing: () => invoke<void>("close_pairing"),
  revokeDevice: (id: string) => invoke<void>("revoke_device", { id }),
  vcpAction: (action: "start" | "stop" | "restart") => invoke<string>("vcp_action", { action }),
  firewallCheck: () => invoke<FirewallStatus>("firewall_check"),
  firewallFix: (allNetworks: boolean) => invoke<FirewallStatus>("firewall_fix", { allNetworks }),
  logs: () => invoke<LogLine[]>("get_logs"),
  watchSystem: (on: boolean) => invoke<void>("watch_system", { on }),
  checkUpdate: () => invoke<UpdateState>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  resetCertificate: () => invoke<Overview>("reset_certificate"),
  diagnostics: () => invoke<unknown>("diagnostics"),
};

/** Internet access for friends (the same card as in the app). */
export const friends: FriendsApi = {
  status: () => invoke<InternetStatus>("internet_status"),
  set: (patch) => invoke<InternetStatus>("set_internet", { patch }),
  test: async () => ({ reachable: await invoke<boolean>("test_internet") }),
  invite: (name, days, charsPerDay, address) => invoke<NewInvite>("create_invite", { name, days, charsPerDay, address }),
  withdraw: (id) => invoke<void>("withdraw_invite", { id }),
  revoke: (id) => invoke<void>("revoke_guest", { id }),
};

/** Error text of a failed command (Rust errors arrive as strings). */
export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
