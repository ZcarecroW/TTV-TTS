// Bridge to the Rust side. Outside Tauri (plain browser) a demo backend
// answers, for UI development, tests and the docs screenshots.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export interface VcpState {
  url: string;
  running: boolean;
  reachable: boolean;
  key_ok: boolean | null;
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
  exe: string | null;
}

export interface ProxyInfo {
  id: string;
  name: string;
  version: string;
  port: number;
  fingerprint: string;
  pairing_open: boolean;
  os: string;
  vcp_reachable: boolean;
  vcp_version: string | null;
}

export interface ProxyStatus {
  info: ProxyInfo;
  vcp: VcpState;
  addresses: { interface: string; ip: string; prefix: number; mac: string }[];
  devices: { id: string; name: string; last_seen: number | null }[];
  connections: number;
  uptime_secs: number;
}

export interface HostStatus {
  kind: "none" | "proxy" | "direct";
  connected: boolean;
  error: string | null;
  proxy: ProxyStatus | null;
  vcp: VcpState | null;
}

export type HostConfig =
  | { kind: "none" }
  | { kind: "proxy"; id: string; name: string; address: string; fingerprint: string; mac: string | null }
  | { kind: "direct"; url: string };

export type GenerationMode = "auto" | "sentences" | "streaming";

export interface Config {
  device_id: string;
  host: HostConfig;
  generation: GenerationMode;
  audio: { device: string | null; volume: number; gap_ms: number };
  language: string;
  theme: string;
  update_channel: string;
  setup_done: boolean;
}

export interface OutputDevice {
  id: string;
  name: string;
  default: boolean;
}

export interface AppView {
  config: Config;
  host: HostStatus;
  devices: OutputDevice[];
  version: string;
  data_dir: string;
}

export interface Found {
  address: string;
  info: ProxyInfo;
}

export interface Probe {
  address: string;
  info: ProxyInfo;
  fingerprint: string;
  fingerprint_short: string;
}

export interface Voice {
  id: string;
  name: string;
  transcript: string;
  voice_type: string;
  duration_seconds: number;
  has_voice_clone_prompt: boolean;
  created_with_model: string | null;
}

export interface Utterance {
  text: string;
  voice: string;
  language: string;
  emotion?: string | null;
  intensity?: string | null;
  gain?: number;
}

export interface SayReport {
  pieces: number;
  first_audio_ms: number;
  total_ms: number;
  audio_secs: number;
  rtf: number;
}

export interface GpuStats {
  name: string;
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
  state: () => invoke<AppView>("get_state"),
  updateSettings: (patch: Partial<Pick<Config, "language" | "theme" | "update_channel" | "generation" | "setup_done">>) => invoke<AppView>("update_settings", { patch }),
  discover: () => invoke<Found[]>("discover_proxies"),
  probe: (address: string) => invoke<Probe>("probe_proxy", { address }),
  pair: (address: string, fingerprint: string, code: string) => invoke<AppView>("pair_proxy", { address, fingerprint, code }),
  useDirect: (url: string, key: string | null) => invoke<AppView>("use_direct", { url, key }),
  forgetHost: () => invoke<AppView>("forget_host"),
  watchHostSystem: (on: boolean) => invoke<void>("watch_host_system", { on }),
  proxyVcpAction: (action: "start" | "stop" | "restart") => invoke<string>("proxy_vcp_action", { action }),
  wakeHost: () => invoke<void>("wake_host"),
  setAudio: (patch: { device?: string | null; volume?: number; gap_ms?: number }) => invoke<AppView>("set_audio", { patch }),
  voices: () => invoke<Voice[]>("list_voices"),
  speakTest: (utterance: Utterance) => invoke<SayReport>("speak_test", { utterance }),
  skip: () => invoke<void>("skip"),
};

export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** VCP's language names, in the order shown in pickers. */
export const LANGUAGES = [
  ["German", "de"],
  ["English", "en"],
  ["Russian", "ru"],
  ["French", "fr"],
  ["Spanish", "es"],
  ["Italian", "it"],
  ["Portuguese", "pt"],
  ["Japanese", "ja"],
  ["Chinese", "zh"],
  ["Korean", "ko"],
] as const;
